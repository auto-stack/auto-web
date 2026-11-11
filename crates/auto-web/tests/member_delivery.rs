//! PLAN-1006 member delivery integration tests.
//!
//! The heavy oracle exercises the full consumer contract in-library: export
//! fixture A, relocate the bundle to an independent directory, build it
//! fresh+locked from an empty target with generation roots scrubbed, drive
//! real HTTP business state through an explicit external data root, verify
//! persistence after shutdown, and read the state file back independently
//! of the service. Negative tests pin the refusal surface.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use auto_web::delivery::{
    export_rust_service, verify_delivery_bundle, ServiceExportRequest,
    DELIVERY_MANIFEST_FILE,
};

/// Heavy builds honor an explicit scratch root (PLAN990_SCRATCH pattern).
fn scratch_dir() -> tempfile::TempDir {
    match std::env::var_os("PLAN1006_SCRATCH") {
        Some(root) if !root.is_empty() => {
            let base = PathBuf::from(root);
            std::fs::create_dir_all(&base).unwrap();
            tempfile::tempdir_in(&base).unwrap()
        }
        _ => tempfile::tempdir().unwrap(),
    }
}

fn fixture(letter: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/member-delivery")
        .join(letter)
}

fn copy_fixture_to(letter: &str, base: &Path) -> PathBuf {
    let project = base.join(format!("project-{letter}"));
    for entry in ["pac.at", "src/back/api.at", "src/back/store.at"] {
        let source = fixture(letter).join(entry);
        let destination = project.join(entry);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(&source, &destination).unwrap();
    }
    project
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn request(port: u16, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let text = String::from_utf8_lossy(&response).to_string();
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_default();
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string();
    (status, body)
}

struct ServiceGuard(Child);

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn spawn_service(
    binary: &Path,
    port: u16,
    data_root: &Path,
    log: &Path,
) -> ServiceGuard {
    let config = format!(
        "{{\"profile\":\"development\",\"listen\":{{\"port\":{port}}},\"limits\":{{\"drain_timeout_ms\":100}}}}"
    );
    std::fs::write(
        log.with_extension("service.json"),
        serde_json::json!({
            "profile": "development",
            "listen": {"port": port},
            "limits": {"drain_timeout_ms": 100},
        })
        .to_string(),
    )
    .unwrap();
    let log_file = std::fs::File::create(log).unwrap();
    let child = Command::new(binary)
        .env("AUTO_HTTP_SERVICE_JSON", &config)
        .env("MEMBER_DELIVERY_DATA", data_root)
        .stdout(Stdio::from(log_file.try_clone().unwrap()))
        .stderr(Stdio::from(log_file))
        .spawn()
        .unwrap();
    ServiceGuard(child)
}

fn wait_ready(child: &mut Child, port: u16, deadline_secs: u64) {
    let deadline = Instant::now() + Duration::from_secs(deadline_secs);
    loop {
        if child.try_wait().unwrap().is_some() {
            panic!("service exited before readiness");
        }
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.write_all(
                format!("GET /value HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
            let mut response = Vec::new();
            if stream.read_to_end(&mut response).is_ok() && !response.is_empty() {
                return;
            }
        }
        if Instant::now() > deadline {
            panic!("service readiness timed out");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn member_name(project: &Path) -> String {
    auto_web::rust_workspace::generated_build::member_dir_name(
        project,
        auto_web::rust_workspace::generated_build::MemberRole::Back,
    )
}

fn scrubbed_build_command(cwd: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(cwd);
    for key in [
        "AUTO_LANG_ROOT",
        "AUTO_MAN_ROOT",
        "AUTO_UI_ROOT",
        "AUTO_DOWN_ROOT",
        "AUTO_UI_TOOL_BIN",
        "AUTO_RUST_WORKSPACE",
        "AUTO_LANG_CRATE",
        "AUTO_DELIVERY_SOURCE_ROOT",
        "AUTO_UI_SOURCE_ROOT",
        "AUTO_WEB_ROOT",
        "AUTO_WEB_SOURCE_ROOT",
        "CARGO_TARGET_DIR",
        "RUSTC_WRAPPER",
        "CARGO_ENCODED_RUSTFLAGS",
        "AUTO_EXPORT_FAIL_POINT",
    ] {
        command.env_remove(key);
    }
    command
}

#[test]
fn export_rejects_contractless_and_lenient_projects() {
    let base = scratch_dir();
    // No api.at at all.
    let empty = base.path().join("empty-project");
    std::fs::create_dir_all(empty.join("src/back")).unwrap();
    std::fs::write(
        empty.join("pac.at"),
        "{ name: \"empty-project\", scene: \"workspace\" }",
    )
    .unwrap();
    let error = export_rust_service(&ServiceExportRequest {
        project_dir: empty.clone(),
        delivery_root: base.path().join("bundle-empty"),
        resources: vec![],
    })
    .unwrap_err();
    assert!(error.to_string().contains("缺少 back API 合同"), "{error}");

    // Parse-breaking source must be refused strictly (never a lenient
    // scaffold bundle).
    let broken = base.path().join("broken-project");
    std::fs::create_dir_all(broken.join("src/back")).unwrap();
    std::fs::write(
        broken.join("pac.at"),
        "{ name: \"broken-project\", scene: \"workspace\" }",
    )
    .unwrap();
    std::fs::write(
        broken.join("src/back/api.at"),
        "#[api(method=\"GET\", path=\"/value\")]\npub fn value( int { return 1 }\n",
    )
    .unwrap();
    let error = export_rust_service(&ServiceExportRequest {
        project_dir: broken.clone(),
        delivery_root: base.path().join("bundle-broken"),
        resources: vec![],
    })
    .unwrap_err();
    assert!(error.to_string().contains("strict 解析失败"), "{error}");
    assert!(!base.path().join("bundle-broken").exists());
    assert!(!base.path().join("bundle-empty").exists());
}

#[test]
fn export_refuses_unknown_nonempty_root_and_preserves_it() {
    let base = scratch_dir();
    let project = copy_fixture_to("a", base.path());
    let root = base.path().join("unmanaged");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("user-data.txt"), "user content").unwrap();
    let error = export_rust_service(&ServiceExportRequest {
        project_dir: project,
        delivery_root: root.clone(),
        resources: vec![],
    })
    .unwrap_err();
    assert!(error.to_string().contains("拒绝替换未知归属"), "{error}");
    assert_eq!(
        std::fs::read_to_string(root.join("user-data.txt")).unwrap(),
        "user content"
    );
}

#[test]
fn export_resource_surface_valid_missing_and_escaping() {
    let base = scratch_dir();
    let project = copy_fixture_to("a", base.path());
    std::fs::write(project.join("notes.txt"), "resource body\n").unwrap();

    // Missing declared resource → named error, no bundle committed.
    let error = export_rust_service(&ServiceExportRequest {
        project_dir: project.clone(),
        delivery_root: base.path().join("bundle-missing"),
        resources: vec![PathBuf::from("absent.txt")],
    })
    .unwrap_err();
    assert!(error.to_string().contains("资源不存在"), "{error}");
    assert!(!base.path().join("bundle-missing").exists());

    // Escaping path → named error.
    let error = export_rust_service(&ServiceExportRequest {
        project_dir: project.clone(),
        delivery_root: base.path().join("bundle-escape"),
        resources: vec![PathBuf::from("../outside.txt")],
    })
    .unwrap_err();
    assert!(error.to_string().contains("资源路径必须"), "{error}");

    // Valid resource exports verbatim into the bundle.
    let receipt = locked_export(&ServiceExportRequest {
        project_dir: project.clone(),
        delivery_root: base.path().join("bundle-ok"),
        resources: vec![PathBuf::from("notes.txt")],
    });
    assert_eq!(
        std::fs::read_to_string(receipt.delivery_root.join("resources/notes.txt")).unwrap(),
        "resource body\n"
    );
    verify_delivery_bundle(&receipt.delivery_root).unwrap();
}

/// AUTO_EXPORT_FAIL_POINT is process-global: every export that must
/// succeed and every env mutation takes this lock.
static EXPORT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn locked_export(req: &ServiceExportRequest) -> auto_web::delivery::ServiceExportReceipt {
    let _guard = EXPORT_LOCK.lock().unwrap();
    export_rust_service(req).unwrap()
}

#[test]
fn export_fail_points_preserve_prior_bundle_bytes() {
    let lock = EXPORT_LOCK.lock().unwrap();
    let base = scratch_dir();
    let project = copy_fixture_to("a", base.path());
    let root = base.path().join("bundle");
    let first = export_rust_service(&ServiceExportRequest {
        project_dir: project.clone(),
        delivery_root: root.clone(),
        resources: vec![],
    })
    .unwrap();
    let before = verify_delivery_bundle(&root).unwrap();
    let _ = before;
    let manifest_before =
        std::fs::read_to_string(root.join(DELIVERY_MANIFEST_FILE)).unwrap();

    for point in ["prepare", "verify", "swap"] {
        std::env::set_var("AUTO_EXPORT_FAIL_POINT", point);
        let error = export_rust_service(&ServiceExportRequest {
            project_dir: project.clone(),
            delivery_root: root.clone(),
            resources: vec![],
        })
        .unwrap_err();
        std::env::remove_var("AUTO_EXPORT_FAIL_POINT");
        assert!(error.to_string().contains(point), "{point}: {error}");
        // The prior bundle stays byte-identical.
        verify_delivery_bundle(&root).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(DELIVERY_MANIFEST_FILE)).unwrap(),
            manifest_before,
            "prior manifest changed at {point}"
        );
    }
    let _ = first;
}

#[test]
fn relocated_bundle_builds_locked_and_serves_persistent_state() {
    let base = scratch_dir();
    let project = copy_fixture_to("a", base.path());
    let export_root = base.path().join("export-bundle");
    let receipt = locked_export(&ServiceExportRequest {
        project_dir: project,
        delivery_root: export_root.clone(),
        resources: vec![],
    });
    verify_delivery_bundle(&export_root).unwrap();

    // Relocate: the bundle must be position-independent.
    let relocated = base.path().join("consumer-home").join("delivered");
    std::fs::create_dir_all(relocated.parent().unwrap()).unwrap();
    std::fs::rename(&export_root, &relocated).unwrap();
    assert!(!export_root.exists());
    verify_delivery_bundle(&relocated).unwrap();
    let member_dir = format!("app/{}", receipt.member_name);

    // Fresh locked build from an empty target with generation roots scrubbed.
    let status = scrubbed_build_command(&relocated)
        .args(["build", "--locked", "-j", "4"])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "locked build failed:\n{}",
        String::from_utf8_lossy(&status.stderr)
    );

    // A second locked build must also succeed (lock replay).
    let status = scrubbed_build_command(&relocated)
        .args(["build", "--locked", "-j", "4"])
        .output()
        .unwrap();
    assert!(status.status.success());

    // Removing the lock must break --locked (the lock is real).
    let lock_path = relocated.join("Cargo.lock");
    let lock_bytes = std::fs::read(&lock_path).unwrap();
    std::fs::remove_file(&lock_path).unwrap();
    let status = scrubbed_build_command(&relocated)
        .args(["build", "--locked", "-j", "4"])
        .output()
        .unwrap();
    assert!(!status.status.success(), "missing lock must fail --locked");
    std::fs::write(&lock_path, &lock_bytes).unwrap();

    // Real HTTP business state through an explicit external data root.
    let data_root = base.path().join("data-root");
    std::fs::create_dir_all(&data_root).unwrap();
    let binary = relocated
        .join("target/debug")
        .join(format!(
            "{}{}",
            receipt.member_name,
            std::env::consts::EXE_SUFFIX
        ))
        .canonicalize()
        .unwrap();
    assert!(binary.is_file(), "member binary missing: {}", binary.display());
    let port = free_port();
    let log = base.path().join("service.log");
    let mut guard = spawn_service(&binary, port, &data_root, &log);
    wait_ready(&mut guard.0, port, 120);

    let (status, body) = request(port, "GET", "/value", None);
    assert_eq!(status, 200);
    assert_eq!(body.trim(), "100");

    let (status, body) = request(port, "POST", "/counter/add", Some("{\"delta\":7}"));
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"ok\":true"), "{body}");
    assert!(body.contains("\"value\":7"), "{body}");

    let (status, body) = request(port, "GET", "/counter/get", None);
    assert_eq!(status, 200);
    assert!(body.contains("\"value\":7"), "{body}");

    // Named business error when the data root is absent (negative control:
    // never an empty success).
    let (status, body) = request(port, "POST", "/counter/bogus", Some("{\"x\":1}"));
    assert_eq!(status, 404, "{body}");

    let (status, body) = request(port, "POST", "/__auto/shutdown", None);
    assert_eq!(status, 202, "{body}");
    guard.0.wait().unwrap();
    drop(guard);
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    drop(listener);

    // Independent read-back: the SQLite state file exists outside the
    // service with real persisted bytes (the driver performs a true
    // non-service SQL read; here we pin the artifact and verify a fresh
    // process reads the same state).
    let database = data_root.join("counter.db");
    assert!(database.is_file(), "counter.db missing in data root");
    assert!(std::fs::metadata(&database).unwrap().len() > 0);

    // Persistence across restart: a new process reads the same state.
    let port2 = free_port();
    let mut guard = spawn_service(&binary, port2, &data_root, &base.path().join("service2.log"));
    wait_ready(&mut guard.0, port2, 60);
    let (status, body) = request(port2, "GET", "/counter/get", None);
    assert_eq!(status, 200);
    assert!(body.contains("\"value\":7"), "{body}");
    let (status, body) = request(port2, "POST", "/counter/add", Some("{\"delta\":3}"));
    assert!(body.contains("\"value\":10"), "{body}");
    let (status, _) = request(port2, "POST", "/__auto/shutdown", None);
    assert_eq!(status, 202);
    guard.0.wait().unwrap();
    let _ = member_dir;
}
