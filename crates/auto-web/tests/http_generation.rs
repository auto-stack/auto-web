//! Cold generation through the public Web entry, real Rust build and exact TCP.
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
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
fn request(port: u16, method: &str, path: &str) -> String {
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(socket,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    response
}
#[test]
fn cold_generated_rust_executes_and_missing_web_preserves_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("web-generation-890");
    std::fs::create_dir_all(project.join("src/back")).unwrap();
    std::fs::write(
        project.join("pac.at"),
        "{ name: \"web-generation-890\", scene: \"workspace\" }",
    )
    .unwrap();
    std::fs::write(
        project.join("src/back/api.at"),
        r#"
#[api(method = "GET", path = "/value")]
pub fn value() int { 890 }
"#,
    )
    .unwrap();
    auto_web::generate_api(&project, "rust").unwrap();
    assert!(auto_web::backend_generation_is_fresh(&project));
    let workspace = auto_web::rust_workspace::resolve_rust_workspace_dir(&project);
    let member = auto_web::rust_workspace::generated_build::member_dir_name(
        &project,
        auto_web::rust_workspace::generated_build::MemberRole::Back,
    );
    let manifest = workspace.join("Cargo.toml");
    let before = std::fs::read(&manifest).unwrap();
    std::env::set_var("AUTO_WEB_ROOT", dir.path().join("absent"));
    let rejected = auto_web::generate_api(&project, "rust");
    std::env::remove_var("AUTO_WEB_ROOT");
    assert!(rejected
        .unwrap_err()
        .to_string()
        .contains("Web producer source"));
    assert_eq!(
        std::fs::read(&manifest).unwrap(),
        before,
        "missing selected source must not rewrite old workspace"
    );
    auto_web::generate_api(&project, "rust").unwrap();
    assert!(auto_web::backend_generation_is_fresh(&project));
    let member_manifest = workspace.join(&member).join("Cargo.toml");
    let document = std::fs::read_to_string(&member_manifest)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    let binary = document["package"]["name"].as_str().unwrap();
    let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .canonicalize()
        .unwrap();
    let output = Command::new("cargo")
        .args(["build", "--manifest-path"])
        .arg(&member_manifest)
        .arg("--target-dir")
        .arg(&target)
        .args(["-j", "4"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Bind the newly materialized lock, then verify reuse against the same real artifact.
    assert!(auto_web::backend_generation_is_fresh(&project));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = format!(
        r#"{{"profile":"development","listen":{{"port":{port}}},"limits":{{"drain_timeout_ms":100}}}}"#
    );
    let exe = target
        .join("debug")
        .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX));
    let mut child = Child(
        Command::new(exe)
            .env("AUTO_HTTP_SERVICE_JSON", config)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "generated server exited before ready"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let response = request(port, "GET", "/value");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(response.split_once("\r\n\r\n").unwrap().1, "890");
    let response = request(port, "POST", "/__auto/shutdown");
    assert!(response.starts_with("HTTP/1.1 202"), "{response}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
}
