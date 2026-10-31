//! Actual package consumer: installed host, exact TCP response and closed process.
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn installed_host_serves_exact_json_and_control_shutdown_joins() {
    configured_file_entry("development");
}

#[test]
fn configured_proxy_file_entry_enforces_host_policy() {
    configured_file_entry("proxy_service");
}

fn configured_file_entry(profile: &str) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("api.at");
    std::fs::write(
        &source,
        r#"#[api(method = "GET", path = "/nonce")]
fn nonce() str { "web-package-runtime-890" }
#[api(method = "GET", path = "/names")]
fn names() []str { return "alpha.txt,照片.png".split(",") }
#[api(method = "GET", path = "/ints")]
fn ints() []int { return [-1, -4, -5] }
"#,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = dir.path().join("service.json");
    let fallback = TcpListener::bind("127.0.0.1:0").unwrap();
    let fallback_port = fallback.local_addr().unwrap().port();
    assert_ne!(port, fallback_port);
    let mut settings = serde_json::json!({"profile":profile,"listen":{"addr":"127.0.0.1","port":port},"limits":{"drain_timeout_ms":100}});
    if profile == "proxy_service" {
        settings["allowed_hosts"] = serde_json::json!(["only.example"]);
        settings["auth"] = serde_json::json!({"responsibility":"app"});
    }
    let settings = settings.to_string();
    let resolved = auto_web::contract::resolve_service_config(&settings, None).unwrap();
    let expected_hash = format!("{:016x}", resolved.effective_config_hash());
    std::fs::write(&config, settings).unwrap();
    drop(fallback);
    let mut child = Child(
        Command::new(env!("CARGO_BIN_EXE_auto-web"))
            .args([
                "run",
                source.to_str().unwrap(),
                "--config",
                config.to_str().unwrap(),
            ])
            .env("AUTO_HTTP_PORT", fallback_port.to_string())
            .env("AUTO_EXECUTION_ENGINE", "autovm")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut socket = loop {
        if let Ok(socket) = TcpStream::connect(("127.0.0.1", port)) {
            break socket;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "Web exited before ready"
        );
        assert!(Instant::now() < deadline, "Web did not become ready");
        std::thread::sleep(Duration::from_millis(20));
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .write_all(b"GET /nonce HTTP/1.1\r\nHost: only.example\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(
        response.split_once("\r\n\r\n").unwrap().1,
        "\"web-package-runtime-890\""
    );
    for (route, expected) in [
        ("/names", serde_json::json!(["alpha.txt", "照片.png"])),
        ("/ints", serde_json::json!([-1, -4, -5])),
    ] {
        let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            socket,
            "GET {route} HTTP/1.1\r\nHost: only.example\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let json: serde_json::Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(json, expected, "{route}");
    }
    assert!(
        TcpStream::connect(("127.0.0.1", fallback_port)).is_err(),
        "configured service must not bind the environment port"
    );
    if profile == "proxy_service" {
        let mut denied = TcpStream::connect(("127.0.0.1", port)).unwrap();
        denied
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        denied
            .write_all(
                b"GET /nonce HTTP/1.1\r\nHost: untrusted.example\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        denied.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    }
    let mut control = TcpStream::connect(("127.0.0.1", port)).unwrap();
    control
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    control.write_all(b"POST /__auto/shutdown HTTP/1.1\r\nHost: only.example\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    let mut reply = String::new();
    control.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 202"), "{reply}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned service failed to join after shutdown"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut diagnostics = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut diagnostics)
        .unwrap();
    assert!(
        diagnostics.contains(&format!(
            "on 127.0.0.1:{port} (profile: {profile}, config_hash: {expected_hash})"
        )),
        "{diagnostics}"
    );
    assert!(!diagnostics.contains("on 0.0.0.0:"), "{diagnostics}");
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "listener retained after owned return"
    );
}

#[test]
fn phase_port_retains_installed_upload_limit_snapshot() {
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => std::env::set_var("AUTO_HTTP_UPLOAD_MAX_WIRE_BYTES", v),
                None => std::env::remove_var("AUTO_HTTP_UPLOAD_MAX_WIRE_BYTES"),
            }
        }
    }
    let _restore = Restore(std::env::var_os("AUTO_HTTP_UPLOAD_MAX_WIRE_BYTES"));
    std::env::set_var("AUTO_HTTP_UPLOAD_MAX_WIRE_BYTES", "64");
    auto_web::http_upload_service::install_upload_executor_service();
    assert_eq!(auto_web::http_upload_service::upload_session_count(), 0);
    std::env::set_var("AUTO_HTTP_UPLOAD_MAX_WIRE_BYTES", "1024");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("public");
    let staging = dir.path().join("staging");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&staging).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let make=||a2r_std::http::upload_request_from_parts("POST","/upload",vec![("content-type".into(),"application/octet-stream".into()),("content-length".into(),"100".into())],Box::pin(futures::stream::iter([Ok(vec![7u8;100])])));
        let session=a2r_std::http::server_upload::upload_receive_with_phase(make(),root.to_str().unwrap(),staging.to_str().unwrap(),r#"{"mode":"raw","max_wire_bytes":256}"#,std::sync::Arc::new(|_|{})).await;
        let admitted=session.is_received();
        let metadata=a2r_std::http::upload_metadata_json(&session);
        if admitted { a2r_std::http::upload_reject(session,400,"test cleanup").await; }
        assert!(!admitted,"phase port reread mutable environment instead of the installed executor snapshot: {metadata}");
        assert!(metadata.contains("413"),"{metadata}");
        // Ordinary facade keeps its existing per-call environment parsing.
        let ordinary=a2r_std::http::upload_receive(make(),root.to_str().unwrap(),staging.to_str().unwrap(),r#"{"mode":"raw","max_wire_bytes":256}"#).await;
        assert!(ordinary.is_received(),"ordinary facade control must retain its existing per-call environment behavior");
        a2r_std::http::upload_reject(ordinary,400,"accepted control cleanup").await;
        assert_eq!(auto_web::http_upload_service::upload_session_count(),0);
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(),0);
    });
}
