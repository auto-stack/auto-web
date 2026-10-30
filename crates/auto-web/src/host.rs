//! Explicit Web host composition for the language SDK.
use crate::*;
use auto_lang_core::vm::host_capabilities::http_service::{
    self, BlockingListenerKind, HttpServiceFuture, HttpServiceHost, HttpVmOwner, VmHttpBridge,
};
use std::sync::Arc;

#[derive(Default)]
pub struct WebHttpServiceHost;
impl HttpServiceHost for WebHttpServiceHost {
    fn serve(&self, owner: HttpVmOwner) -> HttpServiceFuture {
        Box::pin(serve_activated(owner))
    }
    fn listen_blocking(
        &self,
        vm: &vm::engine::AutoVM,
        address: &str,
        metadata: Arc<http_service::messages::HttpProgramMetadata>,
        kind: BlockingListenerKind,
    ) -> AutoResult<()> {
        listen_borrowed(vm, address, metadata, kind, None)
    }
}
pub fn install_http_service_host_if_absent() -> bool {
    http_service::install_process_http_service_host_if_absent(Arc::new(WebHttpServiceHost))
}
/// Run a borrowed listener on its existing VM thread; optional stop is embedding-owned.
pub fn listen_borrowed(
    vm: &vm::engine::AutoVM,
    address: &str,
    metadata: Arc<http_service::messages::HttpProgramMetadata>,
    kind: BlockingListenerKind,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> AutoResult<()> {
    let result = match kind {
        BlockingListenerKind::ServerHandle => {
            http_server::server_blocking::serve_server_handle(vm, address, metadata, stop)
        }
        BlockingListenerKind::LegacyStdnet => {
            http_server::server_legacy::serve_legacy_stdnet(vm, address, metadata, stop)
        }
    };
    result.map_err(|e| error::AutoError::Msg(format!("{e:?}")))
}
/// Explicit embedding assembly with an owned stop receiver and transport settings.
/// The framework owns network/log completion; the VM stays local.
pub async fn serve_owned(
    owner: HttpVmOwner,
    address: &str,
    config: http_transport::TransportConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> AutoResult<()> {
    let (vm, metadata) = owner.into_local_parts();
    http_server::with_owner_metadata(
        metadata,
        http_server::serve_with(vm, address, config, shutdown),
    )
    .await
}
async fn serve_activated(owner: HttpVmOwner) -> AutoResult<()> {
    let (vm, metadata) = owner.into_local_parts();
    let routes: Vec<_> = metadata
        .routes
        .iter()
        .map(|r| (r.method.clone(), r.path.clone(), r.fn_name.clone()))
        .collect();
    http_server::with_owner_metadata(metadata, async move {
        // PLAN-736 AC-01: 显式服务配置面（`auto serve`/run_file 服务装配）——
        // listen 取自解析后的 HttpServiceConfig（默认 loopback），不再读 env；
        // Compatibility input was already snapshotted at invocation entry;
        // activation never takes process-global deployment config.
        let configured = VmHttpBridge::launch(&vm)
            .map(|launch| {
                launch
                    .settings::<crate::http_service_config::HttpServiceConfig>()
                    .ok_or_else(|| {
                        crate::error::AutoError::Msg("unsupported HTTP launch settings type".into())
                    })
            })
            .transpose()?;
        if let Some(svc) = configured {
            // PLAN-736 T-05/AC-02：业务路由占用 /__auto/* 控制面 = 装配诊断
            // （非零退出；不静默遮蔽控制面）。
            let control_conflicts: Vec<&str> = routes
                .iter()
                .map(|r| r.1.as_str())
                .filter(|p| p.starts_with("/__auto/"))
                .collect();
            if !control_conflicts.is_empty() {
                return Err(crate::error::AutoError::Msg(format!(
                    "http service: business routes conflict with the reserved control plane: {:?}",
                    control_conflicts
                )));
            }
            let addr = format!("{}:{}", svc.listen_addr, svc.listen_port);
            let runtime =
                std::sync::Arc::new(crate::http_service_config::ServiceRuntime::new(svc.clone()));
            // PLAN-736 AC-03/05: 预算与观测由服务配置驱动（env 不参与该面）。
            runtime
                .observation
                .install_log_sink(svc.observability.log_sink_capacity);
            runtime.observation.emit_event(serde_json::json!({
                "event": "startup",
                "profile": svc.profile.as_str(),
                "addr": addr,
                "config_hash": format!("{:016x}", svc.effective_config_hash()),
            }));
            eprintln!(
                "[HTTP] Auto-starting server with {} route(s) on {} (profile: {}, config_hash: {:016x})",
                routes.len(), addr, svc.profile.as_str(), svc.effective_config_hash()
            );
            let transport = crate::http_transport::TransportConfig::from_service_runtime(
                runtime.clone(),
            );
            crate::http_server::serve_async_with(vm, &addr, transport).await?;
            return Ok(());
        }
        let port = std::env::var("AUTO_HTTP_PORT").unwrap_or_else(|_| "8080".to_string());
        let addr = format!("0.0.0.0:{}", port);
        eprintln!(
            "[HTTP] Auto-starting server with {} route(s) on {}",
            routes.len(),
            addr
        );

        crate::http_server::serve_async(vm, &addr).await?;

        Ok(())
    }).await
}
