//! AutoWeb foundation. Runtime/generator ownership moves in PLAN-890 T-05/06.
pub use auto_web_contract as contract;

/// Bind domain-owned, resolved settings to an invocation without selecting its host.
pub fn service_launch(
    config: std::sync::Arc<contract::HttpServiceConfig>,
) -> auto_lang_core::vm::host_capabilities::http_service::HttpServiceLaunch {
    auto_lang_core::vm::host_capabilities::http_service::HttpServiceLaunch::new(config)
}
