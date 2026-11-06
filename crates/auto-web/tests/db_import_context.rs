//! Actual public generation, Rust compilation and HTTP execution of imported records.
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

struct OwnedChild(std::process::Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn request(port: u16, method: &str, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(s,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    let mut result = String::new();
    s.read_to_string(&mut result).unwrap();
    result
}

#[test]
fn p990_db_imported_string_records_compile_and_execute() {
    let scratch = std::env::var_os("PLAN990_SCRATCH").map(PathBuf::from);
    let dir = match scratch {
        Some(p) => {
            std::fs::create_dir_all(&p).unwrap();
            tempfile::tempdir_in(p).unwrap()
        }
        None => tempfile::tempdir().unwrap(),
    };
    let root = dir.path().join("db-import-context");
    std::fs::create_dir_all(root.join("src/back")).unwrap();
    std::fs::write(
        root.join("pac.at"),
        "{ name: \"db-import-context\", scene: \"workspace\" }",
    )
    .unwrap();
    std::fs::write(
        root.join("src/back/api.at"),
        r#"
use db
pub type Entry = { value: str, n: int }
pub type Number = { value: int }
#[api(method = "GET", path = "/records")]
pub fn records(seed str) []Entry { return db.records(seed) }
#[api(method = "GET", path = "/number")]
pub fn number(value int) int { return db.numeric(value) }
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/back/db.at"),
        r#"
use api: Entry, Number
pub fn records(seed str) List<Entry> {
    let token str = seed + "-owned"
    var rows List<Entry> = []
    for i in [1, 2] {
        rows.push(Entry { value: token, n: i })
    }
    let tail Entry = Entry { value: seed, n: 3 }
    rows.push(tail)
    let literal str = "token: token, value: value }"
    // value: value, token: token } must not become semantic code.
    rows.push(Entry { value: literal, n: 4 })
    return rows
}
pub fn numeric(value int) int {
    let result Number = Number { value: value }
    return result.value
}
"#,
    )
    .unwrap();
    auto_web::generate_api(&root, "rust").unwrap();
    let workspace = auto_web::rust_workspace::resolve_rust_workspace_dir(&root);
    let member = auto_web::rust_workspace::generated_build::member_dir_name(
        &root,
        auto_web::rust_workspace::generated_build::MemberRole::Back,
    );
    let manifest = workspace.join(&member).join("Cargo.toml");
    let document = std::fs::read_to_string(&manifest)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    let binary = document["package"]["name"].as_str().unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.path().join("target"));
    let build = Command::new("cargo")
        .args(["build", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .args(["-j", "2"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = format!(
        r#"{{"profile":"development","listen":{{"port":{port}}},"limits":{{"drain_timeout_ms":100}}}}"#
    );
    let mut child = OwnedChild(
        Command::new(
            target
                .join("debug")
                .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX)),
        )
        .env("AUTO_HTTP_SERVICE_JSON", config)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "generated server exited before ready"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let response = request(port, "GET", "/records?seed=%E4%B8%AD%E6%96%87");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let actual: serde_json::Value =
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        actual,
        serde_json::json!([
        {"value":"中文-owned","n":1},{"value":"中文-owned","n":2},
        {"value":"中文","n":3},{"value":"token: token, value: value }","n":4}])
    );
    let response = request(port, "GET", "/number?value=7");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(response.split_once("\r\n\r\n").unwrap().1, "7");
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
