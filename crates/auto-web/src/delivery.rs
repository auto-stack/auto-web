//! PLAN-1006: relocatable delivery bundles for generated Rust service
//! members.
//!
//! [`export_rust_service`] produces a self-contained bundle for a normal
//! Auto project's generated Rust service member: the freshly generated
//! member crate, the validated SDK source closure (crates + second-level
//! vendor trees + compile-time root inputs), an explicit resource set, a
//! bundle-local `Cargo.lock` and a versioned delivery manifest. Generation
//! itself keeps its normal development behavior — the semantic root stays
//! the original project; only the delivered output is redirected through a
//! staged, verified, atomically swapped directory transaction (neutral
//! `auto_generation_core::commit` primitives).
//!
//! Boundaries: no UI/Man/Tauri dependency, no installer scripting, no
//! process-environment mutation (source selection follows the established
//! explicit env contract), user HOME/APPDATA is never read.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use auto_generation_core::commit::{
    commit_managed_directory, copy_tree_clean, file_map, sha256_bytes, sha256_file,
};
use auto_generation_core::generated_build;
use auto_generation_core::rust_workspace::{select_language_producer, WorkspaceRequirements};
use auto_generation_core::source_owner::local_build_closure;
use auto_val::AutoResult;

use crate::api_gen::generate_api;
use crate::rust_workspace::web_source;

/// Bundle manifest file (also the managed-payload marker for the commit
/// transaction: a non-empty delivery root without a valid one is refused).
pub const DELIVERY_MANIFEST_FILE: &str = "delivery-manifest.json";
const DELIVERY_MANIFEST_SCHEMA: &str = "web-member-delivery/1";
const DELIVERY_VERSION: &str = "0.1.0";
/// Optional injected transaction failure (`prepare`/`verify`/`swap`) for
/// tests and recovery drills — never set in production flows.
const FAIL_POINT_ENV: &str = "AUTO_EXPORT_FAIL_POINT";
/// Crates a member build may never drag into a service bundle.
const FORBIDDEN_PACKAGES: [&str; 5] = ["auto-lang", "auto-api-gen", "tauri", "auto-man", "auto-ui"];
/// Second-level vendor trees mirrored same-shape at the sdk root (PLAN-1003
/// T-03 precedent): package name → path relative to the language producer
/// root. Intentionally NOT `crates/<name>` layout, so they are excluded
/// from the projected sdk member set.
const VENDOR_TREES: [(&str, &str); 2] = [
    (
        "ace-semantics-contract",
        "experimental/ace/crates/semantics-contract",
    ),
    ("autodown-test-parser", "vendor/autodown-test-parser"),
];
/// Repo-root compile-time inputs mirrored into the bundle (man-install
/// ROOT_INPUTS contract + the PLAN-1003 T-05 stdlib contracts).
const ROOT_INPUT_FILES: [&str; 7] = [
    "stdlib/assembly-providers.json",
    "stdlib/auto/image.at",
    "stdlib/auto/image.rs.at",
    "stdlib/auto/image.vm.at",
    "stdlib/contracts/operations.toml",
    "stdlib/contracts/compatibility.toml",
    "docs/schema/aura.at",
];
const ROOT_INPUT_DIRS: [&str; 1] = ["stdlib/auto"];
/// Data-only trees mirrored into the sdk that belong to the facade crate's
/// test corpus: `auto-lang-core` embeds `crates/auto-lang/test/cases/**`
/// inputs at compile time (PLAN-888 governance / ACE M1 oracle). The
/// facade crate itself is still never shipped as a package.
const DATA_TREES: [&str; 1] = ["crates/auto-lang/test/cases"];
/// Environment keys scrubbed from bundle-local cargo invocations (mirror of
/// the Man installer SCRUB contract — generation roots must not leak into
/// the delivered build).
const CARGO_SCRUB_ENV: [&str; 13] = [
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
];

/// Export request: semantic root, explicit output location and an explicit
/// resource set (paths relative to the project).
#[derive(Debug, Clone)]
pub struct ServiceExportRequest {
    pub project_dir: PathBuf,
    pub delivery_root: PathBuf,
    pub resources: Vec<PathBuf>,
}

/// Verifiable outcome of a successful export.
#[derive(Debug, Clone)]
pub struct ServiceExportReceipt {
    pub delivery_root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_sha256: String,
    pub member_name: String,
    pub sdk_packages: Vec<String>,
    pub lock_sha256: String,
    pub debug_policy: String,
    pub backup_root: Option<PathBuf>,
}

/// Resolved, validated export inputs — everything the staging phase needs,
/// frozen before any bundle file is written.
struct ExportPlan {
    project: PathBuf,
    member_name: String,
    member_dir: PathBuf,
    lang_root: PathBuf,
    lang_origin: String,
    web_root: PathBuf,
    /// package name → (installed path inside the bundle, source repo) for
    /// `crates/<name>` layout members of the projected sdk workspace.
    packages: BTreeMap<String, (String, String)>,
    /// Vendor-tree packages present in the closure (mirrored same-shape).
    vendors: Vec<&'static str>,
    debug_policy: String,
    profile_block: String,
    policy_source: String,
    sources: BTreeMap<String, String>,
    /// Absolute development paths that must never appear inside a bundle.
    dev_path_guards: Vec<PathBuf>,
    resources: Vec<PathBuf>,
}

/// Split the local build closure into sdk member crates and vendor trees;
/// reject forbidden packages and unrecognized layouts.
fn classify_closure(
    closure: &BTreeMap<String, PathBuf>,
    lang_root: &Path,
    web_crate: &Path,
) -> AutoResult<(BTreeMap<String, (String, String)>, Vec<&'static str>)> {
    let mut packages = BTreeMap::new();
    let mut vendors = Vec::new();
    let lang_root = lang_root
        .canonicalize()
        .unwrap_or_else(|_| lang_root.to_path_buf());
    let web_crate = web_crate
        .canonicalize()
        .unwrap_or_else(|_| web_crate.to_path_buf());
    for (name, dir) in closure {
        // `auto-lang` (facade) is exact; UI/Man families are prefixes.
        // Language SDK crates (auto-lang-core …) are legitimate members.
        if FORBIDDEN_PACKAGES.contains(&name.as_str())
            || name.starts_with("auto-ui")
            || name.starts_with("auto-man")
        {
            return Err(format!("服务成员闭包含禁入包 {name}（UI/Man/门面/桌面）").into());
        }
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        if canonical == web_crate {
            packages.insert(name.clone(), (format!("crates/{name}"), "auto-web".to_string()));
            continue;
        }
        let rel = canonical
            .strip_prefix(&lang_root)
            .map_err(|_| format!("闭包源不属语言 producer：{name} -> {}", canonical.display()))?;
        let rel_posix = rel.to_string_lossy().replace('\\', "/");
        if rel_posix == format!("crates/{name}") {
            packages.insert(name.clone(), (rel_posix, "auto-lang".to_string()));
            continue;
        }
        if let Some((vendor, _)) = VENDOR_TREES.iter().find(|(vendor, _)| vendor == name) {
            vendors.push(*vendor);
            continue;
        }
        return Err(format!("闭包源布局不识别：{name} -> {rel_posix}").into());
    }
    Ok((packages, vendors))
}

fn git_head(root: &Path) -> AutoResult<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        _ => {}
    }
    // Pinned source copies (driver drills) carry no .git; their provenance
    // marker records the revision the copy was taken from. Content identity
    // is still pinned by the manifest's per-file hash map.
    if let Ok(marker) = fs::read_to_string(root.join(".source-revision")) {
        let revision = marker.trim();
        if revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(revision.to_string());
        }
        return Err(format!(
            "源版本标记无效（.source-revision）: {}",
            root.display()
        )
        .into());
    }
    Err(format!("无法确定源版本（git HEAD / .source-revision）: {}", root.display()).into())
}

fn plan_export(req: &ServiceExportRequest) -> AutoResult<ExportPlan> {
    let project = std::path::absolute(&req.project_dir)
        .map_err(|e| format!("项目路径无效 {}: {e}", req.project_dir.display()))?;
    if !project.is_dir() {
        return Err(format!("项目目录不存在: {}", project.display()).into());
    }

    // Prevalidate the contract before anything is generated: export is
    // strict-only — lenient scaffold placeholders, VM-only projects and
    // contract-less projects are named errors, never empty bundles.
    let api_file = auto_lang_core::config::resolve_back_api(&project).ok_or(
        "导出失败：项目缺少 back API 合同（src/back/api.at / back/api.at / pac.at back 声明）",
    )?;
    let api_content = fs::read_to_string(&api_file)
        .map_err(|e| format!("读取 API 合同失败 {}: {e}", api_file.display()))?;
    let module = auto_generation_core::shared_generation::try_full_parse(&api_content)
        .ok_or("导出失败：api.at strict 解析失败（lenient/占位产物不允许导出；修复解析错误后重试）")?;
    if module.endpoints.is_empty() && module.types.is_empty() {
        return Err("导出失败：api.at 无端点与类型（VM-only 或空合同不允许导出）".into());
    }

    // Resources validate before any generation side effect: a bad request
    // must not touch the development workspace either.
    let resources = validate_resources(&project, &req.resources)?;

    // Normal development generation — same behavior as `auto service` /
    // web flows; the semantic root and landing position stay untouched.
    generate_api(&project, "rust")?;
    if !crate::api_gen::backend_generation_is_fresh(&project) {
        return Err("导出失败：生成收据不新鲜（generation.json 与当前源不一致）".into());
    }
    let ws_dir = crate::rust_workspace::resolve_rust_workspace_dir(&project);
    let member_name = generated_build::member_dir_name(&project, generated_build::MemberRole::Back);
    let member_dir = ws_dir.join(&member_name);
    for required in ["Cargo.toml", "src/main.rs", "generation.json"] {
        if !member_dir.join(required).is_file() {
            return Err(format!(
                "导出失败：服务成员不完整（缺 {required}）：{}",
                member_dir.display()
            )
            .into());
        }
    }

    // Validated source selections (explicit env contract → adjacent SDK →
    // discovery → compiled-in; never rewritten here).
    let selection = select_language_producer(&project, WorkspaceRequirements::Service)?;
    let lang_root = selection.producer_root();
    let web_crate = web_source()?;
    let web_root = web_crate
        .parent()
        .and_then(Path::parent)
        .ok_or("Web 源根路径无效")?
        .to_path_buf();

    let roots = [
        lang_root.join("crates/auto-lang-core"),
        lang_root.join("crates/a2r-std"),
        lang_root.join("crates/auto-media-core"),
        web_crate.clone(),
    ];
    let root_refs: Vec<&Path> = roots.iter().map(|path| path.as_path()).collect();
    let closure = local_build_closure(&root_refs)?;
    let (packages, vendors) = classify_closure(&closure, &lang_root, &web_crate)?;

    // Debug policy resolves before any delivered file is written (PLAN-1003
    // semantics); the bundle records the static policy label only.
    let policy = auto_generation_core::generated_build::resolve_debug_policy()?;
    let profile_block = auto_generation_core::generated_build::managed_profile_toml(policy.policy);

    let sources = BTreeMap::from([
        ("auto-lang".to_string(), git_head(&lang_root)?),
        ("auto-web".to_string(), git_head(&web_root)?),
    ]);

    Ok(ExportPlan {
        dev_path_guards: vec![
            project.clone(),
            member_dir.clone(),
            ws_dir.clone(),
            lang_root.clone(),
            web_root.clone(),
        ],
        project,
        member_name,
        member_dir,
        lang_root,
        lang_origin: selection.origin,
        web_root,
        packages,
        vendors,
        debug_policy: policy.policy.as_str().to_string(),
        profile_block,
        policy_source: policy.source,
        sources,
        resources,
    })
}

/// Explicit resources only: project-internal plain files, no escapes, no
/// reparse points, no VCS/build trees.
fn validate_resources(project: &Path, resources: &[PathBuf]) -> AutoResult<Vec<PathBuf>> {
    let project = project
        .canonicalize()
        .map_err(|e| format!("项目路径无效 {}: {e}", project.display()))?;
    let mut validated = Vec::new();
    for relative in resources {
        let text = relative.to_string_lossy();
        if relative.is_absolute() || text.contains("..") || text.starts_with('/') {
            return Err(format!("资源路径必须是项目内相对路径: {text}").into());
        }
        let source = project.join(relative);
        let canonical = source
            .canonicalize()
            .map_err(|e| format!("资源不存在 {}: {e}", source.display()))?;
        if !canonical.starts_with(&project) {
            return Err(format!("资源越出项目目录: {text}").into());
        }
        let meta = fs::symlink_metadata(&canonical)
            .map_err(|e| format!("资源 stat 失败 {}: {e}", canonical.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!("资源禁止 junction/symlink: {text}").into());
        }
        if !canonical.is_file() {
            return Err(format!("资源必须是普通文件: {text}").into());
        }
        for forbidden in [".git", "target", "node_modules"] {
            if canonical
                .components()
                .any(|component| component.as_os_str() == forbidden)
            {
                return Err(format!("资源位于排除目录 {forbidden}/ 下: {text}").into());
            }
        }
        validated.push(relative.clone());
    }
    Ok(validated)
}

fn generate_lockfile(stage: &Path) -> AutoResult<()> {
    let mut command = std::process::Command::new("cargo");
    command
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg("Cargo.toml")
        .current_dir(stage);
    for key in CARGO_SCRUB_ENV {
        command.env_remove(key);
    }
    command.env_remove("CARGO_ENCODED_RUSTFLAGS");
    let output = command
        .output()
        .map_err(|e| format!("cargo generate-lockfile 启动失败: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "bundle Cargo.lock 生成失败（exit {}）：{}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    if !stage.join("Cargo.lock").is_file() {
        return Err("bundle Cargo.lock 生成后缺失".into());
    }
    Ok(())
}

/// Rewrite cross-repo path anchors inside a copied crate manifest so every
/// dependency resolves within the bundle's `sdk/crates` sibling layout.
fn rewrite_crate_manifest(manifest: &Path) -> AutoResult<()> {
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("读取清单失败 {}: {e}", manifest.display()))?;
    let rewritten = text
        .replace("../../../auto-lang/crates/", "../")
        .replace("../../../auto-web/crates/", "../")
        .replace("../../../auto-ui/crates/", "../");
    if rewritten != text {
        fs::write(manifest, &rewritten).map_err(|e| format!("写回清单失败: {e}"))?;
    }
    Ok(())
}

/// Project the language workspace manifest onto the closure member set
/// (stage_source contract): members replaced, patch section cut,
/// `[workspace.package]` pinned so `version.workspace = true` resolves.
fn project_sdk_workspace(
    lang_root: &Path,
    packages: &BTreeMap<String, (String, String)>,
) -> AutoResult<String> {
    let text = fs::read_to_string(lang_root.join("Cargo.toml"))?;
    let members = packages
        .values()
        .map(|(installed, _)| format!("    \"{installed}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let (mut projected, count) = replace_workspace_members(&text, &format!("members = [\n{members}\n]"))?;
    if count != 1 {
        return Err("语言 workspace members 投影失败（未找到 members 块）".into());
    }
    projected = projected
        .split("[patch.crates-io]")
        .next()
        .unwrap_or("")
        .trim_end()
        .to_string();
    projected.push_str("\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\n");
    Ok(projected)
}

/// Replace the `members = [ ... ]` block (comment/entry lines dropped),
/// keeping exactly one replacement.
fn replace_workspace_members(text: &str, replacement: &str) -> AutoResult<(String, usize)> {
    let mut result = String::new();
    let mut count = 0usize;
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        if !inside && line.trim_start().starts_with("members = [") {
            result.push_str(replacement);
            result.push('\n');
            count += 1;
            inside = true;
            continue;
        }
        if inside {
            if line.starts_with(']') {
                inside = false;
                continue;
            }
            continue;
        }
        result.push_str(line);
    }
    if inside {
        return Err("语言 workspace members 块未闭合".into());
    }
    Ok((result, count))
}

/// Drop machine-local anchors from copied member receipts while keeping
/// every semantic identity field (endpoints, source hashes, lock identity).
fn sanitize_member_receipts(member_stage: &Path) -> AutoResult<()> {
    let generation = member_stage.join("generation.json");
    if generation.is_file() {
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&generation)?)
                .map_err(|e| format!("generation.json 解析失败: {e}"))?;
        doc["build_context"] = serde_json::Value::Null;
        fs::write(&generation, serde_json::to_string_pretty(&doc)? + "\n")?;
    }
    let member = member_stage.join(generated_build::MEMBER_FILE);
    if member.is_file() {
        let mut doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&member)?)
            .map_err(|e| format!("{} 解析失败: {e}", generated_build::MEMBER_FILE))?;
        if let Some(path) = doc.get("project_path").and_then(|value| value.as_str()) {
            let basename = Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            doc["project_path"] = serde_json::Value::String(String::new());
            doc["project_basename"] = serde_json::Value::String(basename);
        }
        fs::write(&member, serde_json::to_string_pretty(&doc)? + "\n")?;
    }
    Ok(())
}

/// Bundle root manifest: the member as the only workspace member, the sdk
/// excluded (nested crates resolve as path dependencies), dependency
/// anchors pointing inside the bundle, C/full profile block preserved.
fn bundle_workspace_manifest(plan: &ExportPlan) -> String {
    format!(
        r#"[workspace]
members = ["app/{member}"]
exclude = ["sdk"]
resolver = "2"

[workspace.dependencies]
auto-lang = {{ path = "sdk/crates/auto-lang" }}
a2r-std = {{ path = "sdk/crates/a2r-std" }}
auto-media-core = {{ path = "sdk/crates/auto-media-core" }}
auto-web = {{ path = "sdk/crates/auto-web", version = "=0.1.0" }}
auto-lang-core = {{ path = "sdk/crates/auto-lang-core" }}
serde_json = "1"
ureq = {{ version = "2", features = ["json"] }}
tokio = {{ version = "1", features = ["rt"] }}
axum = "0.7"
hyper = {{ version = "1", features = ["http1", "server"] }}
hyper-util = {{ version = "0.1", features = ["tokio", "server-auto", "service", "http1"] }}
serde = {{ version = "1", features = ["derive"] }}
tower-http = {{ version = "0.5", features = ["cors"] }}
{profile_block}"#,
        member = plan.member_name,
        profile_block = plan.profile_block,
    )
}

fn stage_payload(plan: &ExportPlan, stage: &Path) -> AutoResult<()> {
    let sdk = stage.join("sdk");
    // 1. Closure crates with rewritten cross-repo anchors.
    for (name, (installed, repo)) in &plan.packages {
        let source = if repo == "auto-web" {
            plan.web_root.join("crates").join(name)
        } else {
            plan.lang_root.join(installed)
        };
        let destination = sdk.join(installed);
        fs::create_dir_all(destination.parent().unwrap())
            .map_err(|e| format!("创建 sdk 目录失败: {e}"))?;
        copy_tree_clean(&source, &destination)
            .map_err(|e| format!("闭包 crate {name} 拷贝失败: {e}"))?;
        rewrite_crate_manifest(&destination.join("Cargo.toml"))?;
    }
    // 2. Second-level vendor trees (same-shape, no member projection).
    for package in &plan.vendors {
        let (_, rel) = VENDOR_TREES.iter().find(|(name, _)| name == package).unwrap();
        let source = plan.lang_root.join(rel);
        let destination = sdk.join(rel);
        fs::create_dir_all(destination.parent().unwrap())
            .map_err(|e| format!("创建 vendor 目录失败: {e}"))?;
        copy_tree_clean(&source, &destination)
            .map_err(|e| format!("vendor 树 {package} 拷贝失败: {e}"))?;
    }
    // 3. Compile-time repo-root inputs — directories first (man-install
    // owner-contract order): copy_tree_clean requires a fresh destination,
    // and the individual files may live inside the copied directories.
    for rel in ROOT_INPUT_DIRS {
        let source = plan.lang_root.join(rel);
        if source.is_dir() {
            copy_tree_clean(&source, &sdk.join(rel))
                .map_err(|e| format!("root input 目录 {rel} 拷贝失败: {e}"))?;
        }
    }
    for rel in ROOT_INPUT_FILES {
        let destination = sdk.join(rel);
        fs::create_dir_all(destination.parent().unwrap())
            .map_err(|e| format!("创建 root input 目录失败: {e}"))?;
        fs::copy(plan.lang_root.join(rel), &destination)
            .map_err(|e| format!("root input {rel} 拷贝失败: {e}"))?;
    }
    // 3b. Compile-time data trees (facade test corpus).
    for rel in DATA_TREES {
        let source = plan.lang_root.join(rel);
        let destination = sdk.join(rel);
        fs::create_dir_all(destination.parent().unwrap())
            .map_err(|e| format!("创建 data tree 目录失败: {e}"))?;
        copy_tree_clean(&source, &destination)
            .map_err(|e| format!("data tree {rel} 拷贝失败: {e}"))?;
    }
    // 4. Licenses.
    for (repo, root) in [("auto-lang", &plan.lang_root), ("auto-web", &plan.web_root)] {
        let destination = sdk.join("licenses").join(repo);
        fs::create_dir_all(&destination).map_err(|e| format!("创建 licenses 失败: {e}"))?;
        fs::copy(root.join("LICENSE"), destination.join("LICENSE"))
            .map_err(|e| format!("{repo} LICENSE 拷贝失败: {e}"))?;
    }
    // 5. Projected sdk workspace manifest. auto-lang-core's build script
    // hashes the repo-root Cargo.toml AND Cargo.lock into its build
    // identity, so the sdk carries the producer's lock verbatim (the sdk
    // workspace itself is never resolved by consumers — its crates build as
    // path dependencies of the bundle root).
    fs::write(
        sdk.join("Cargo.toml"),
        project_sdk_workspace(&plan.lang_root, &plan.packages)?,
    )?;
    fs::copy(plan.lang_root.join("Cargo.lock"), sdk.join("Cargo.lock"))
        .map_err(|e| format!("sdk Cargo.lock 拷贝失败: {e}"))?;
    // 6. The generated member (receipts sanitized to drop dev anchors).
    let member_stage = stage.join("app").join(&plan.member_name);
    copy_tree_clean(&plan.member_dir, &member_stage)
        .map_err(|e| format!("成员 crate 拷贝失败: {e}"))?;
    sanitize_member_receipts(&member_stage)?;
    // 7. Bundle root manifest + cargo config (local target).
    fs::write(stage.join("Cargo.toml"), bundle_workspace_manifest(plan))?;
    let cargo_dir = stage.join(".cargo");
    fs::create_dir_all(&cargo_dir)?;
    fs::write(
        cargo_dir.join("config.toml"),
        "[build]\ntarget-dir = \"target\"\n\n[target.x86_64-pc-windows-msvc]\nrustflags = [\"-C\", \"link-arg=/STACK:268435456\"]\n",
    )?;
    // 8. Explicit resources.
    for relative in &plan.resources {
        let destination = stage.join("resources").join(relative);
        fs::create_dir_all(destination.parent().unwrap())
            .map_err(|e| format!("创建资源目录失败: {e}"))?;
        fs::copy(plan.project.join(relative), &destination)
            .map_err(|e| format!("资源 {} 拷贝失败: {e}", relative.display()))?;
    }
    // 9. Bundle-local lock, then the delivery manifest.
    generate_lockfile(stage)?;
    write_delivery_manifest(plan, stage)?;
    Ok(())
}

fn write_delivery_manifest(plan: &ExportPlan, stage: &Path) -> AutoResult<()> {
    let files = file_map(stage, &delivery_map_skips())?;
    let lock_sha256 = sha256_file(&stage.join("Cargo.lock"))?;
    let files_json: serde_json::Map<String, serde_json::Value> = files
        .iter()
        .map(|(rel, record)| (rel.clone(), record.to_json()))
        .collect();
    let files_sha256 = sha256_bytes(serde_json::to_string(&files_json)?.as_bytes());
    let document = serde_json::json!({
        "schema": DELIVERY_MANIFEST_SCHEMA,
        "version": DELIVERY_VERSION,
        "sources": plan.sources,
        "member": { "name": plan.member_name },
        "source_closure": {
            "sdk_layout": "combined-crates",
            "packages": plan.packages.iter().map(|(name, (installed, repo))| serde_json::json!({
                "name": name, "source_repo": repo,
                "source_path": installed, "installed_path": installed,
            })).collect::<Vec<_>>(),
            "vendor_trees": plan.vendors.iter().map(|package| {
                let (_, rel) = VENDOR_TREES.iter().find(|(name, _)| name == package).unwrap();
                serde_json::json!({"package": package, "source_repo": "auto-lang",
                                   "source_path": rel, "installed_path": rel})
            }).collect::<Vec<_>>(),
            "root_inputs": ROOT_INPUT_FILES.iter().chain(ROOT_INPUT_DIRS.iter())
                .chain(DATA_TREES.iter())
                .map(|rel| serde_json::json!({"path": rel, "source_repo": "auto-lang"}))
                .collect::<Vec<_>>(),
        },
        "debug_policy": {
            "label": plan.debug_policy,
            "source": plan.policy_source,
            "generation_env_override_not_carried": true,
        },
        "lock_sha256": lock_sha256,
        "exporter": {
            "crate": "auto-web", "version": DELIVERY_VERSION,
            "entry": "export_rust_service",
            "producer_origin": plan.lang_origin,
        },
        "receipts_sanitized": true,
        "files": files_json,
        "files_sha256": files_sha256,
    });
    fs::write(
        stage.join(DELIVERY_MANIFEST_FILE),
        serde_json::to_string_pretty(&document)? + "\n",
    )?;
    Ok(())
}

/// Files excluded from the delivery content map: the manifest itself and
/// the neutral staging marker (present only while the transaction is
/// populating the tree, removed before the swap).
fn delivery_map_skips() -> Vec<&'static str> {
    vec![
        DELIVERY_MANIFEST_FILE,
        auto_generation_core::commit::STAGING_MARKER_FILE,
    ]
}

/// Strict verification of a delivery payload directory — used for the
/// staged bundle before the swap, the committed root after the swap, and
/// any prior bundle during managed-payload checks.
pub fn verify_delivery_bundle(root: &Path) -> AutoResult<()> {
    let manifest_path = root.join(DELIVERY_MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("交付清单缺失 {}: {e}", manifest_path.display()))?;
    let document: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("交付清单解析失败: {e}"))?;
    if document["schema"].as_str() != Some(DELIVERY_MANIFEST_SCHEMA)
        || document["version"].as_str() != Some(DELIVERY_VERSION)
    {
        return Err(format!(
            "非本交付合同的目录（schema={:?}）：{}",
            document["schema"].as_str(),
            root.display()
        )
        .into());
    }
    let actual = file_map(root, &delivery_map_skips())?;
    let expected = document["files"]
        .as_object()
        .ok_or("交付清单 files 结构无效")?;
    if actual.len() != expected.len() {
        return Err(format!(
            "交付内容与清单不符：清单 {} 项，实际 {} 项（{}）",
            expected.len(),
            actual.len(),
            root.display()
        )
        .into());
    }
    for (rel, record) in &actual {
        let listed = expected
            .get(rel)
            .ok_or_else(|| format!("交付清单缺文件 {rel}"))?;
        if listed["bytes"].as_u64() != Some(record.bytes)
            || listed["sha256"].as_str() != Some(record.sha256.as_str())
        {
            return Err(format!("交付文件与清单不符: {rel}").into());
        }
    }
    let member = document["member"]["name"].as_str().unwrap_or_default();
    for required in [
        "Cargo.toml",
        "Cargo.lock",
        ".cargo/config.toml",
        "sdk/Cargo.toml",
        "sdk/crates/auto-web/Cargo.toml",
        "sdk/crates/auto-lang-core/Cargo.toml",
        &format!("app/{member}/Cargo.toml"),
        &format!("app/{member}/src/main.rs"),
    ] {
        if !root.join(required).is_file() {
            return Err(format!("交付必备件缺失: {required}").into());
        }
    }
    Ok(())
}

/// Scan bundle manifests/configs/sources for leaked development absolute
/// paths — a relocatable bundle must never anchor back to a producer
/// checkout.
fn verify_no_dev_anchors(stage: &Path, plan: &ExportPlan) -> AutoResult<()> {
    let files = file_map(stage, &delivery_map_skips())?;
    let mut scanned = 0usize;
    for rel in files.keys() {
        if !(rel.ends_with(".toml") || rel.ends_with(".json") || rel.ends_with(".rs")) {
            continue;
        }
        let text = fs::read_to_string(stage.join(rel))
            .map_err(|e| format!("读取 {rel} 失败: {e}"))?;
        scanned += 1;
        for guard in &plan.dev_path_guards {
            let needle = guard.to_string_lossy();
            if text.contains(needle.as_ref()) || text.contains(&needle.replace('\\', "/")) {
                return Err(format!(
                    "bundle 内 {rel} 泄漏开发根路径 {}——不可迁移",
                    guard.display()
                )
                .into());
            }
        }
    }
    if scanned == 0 {
        return Err("bundle 无可扫描清单文件（结构异常）".into());
    }
    Ok(())
}

/// Export a generated Rust service member into a relocatable delivery
/// bundle at `delivery_root`.
///
/// See the module documentation for the contract. Failures preserve any
/// existing bundle byte-for-byte, name their phase and every preserved
/// artifact, and never claim an unmanaged non-empty directory.
pub fn export_rust_service(req: &ServiceExportRequest) -> AutoResult<ServiceExportReceipt> {
    let delivery_root = std::path::absolute(&req.delivery_root)
        .map_err(|e| format!("交付根路径无效 {}: {e}", req.delivery_root.display()))?;
    let plan = plan_export(req)?;
    let fail_point = std::env::var(FAIL_POINT_ENV).ok().filter(|v| !v.is_empty());
    let outcome = commit_managed_directory(
        &delivery_root,
        "web-member-delivery",
        &verify_delivery_bundle,
        &|stage| stage_payload(&plan, stage),
        &|stage| {
            verify_delivery_bundle(stage)?;
            verify_no_dev_anchors(stage, &plan)
        },
        fail_point.as_deref(),
    )?;
    let manifest_path = delivery_root.join(DELIVERY_MANIFEST_FILE);
    Ok(ServiceExportReceipt {
        member_name: plan.member_name.clone(),
        sdk_packages: plan.packages.keys().cloned().collect(),
        debug_policy: plan.debug_policy.clone(),
        manifest_sha256: sha256_file(&manifest_path)?,
        lock_sha256: sha256_file(&delivery_root.join("Cargo.lock"))?,
        manifest_path,
        delivery_root: outcome.root,
        backup_root: outcome.backup_root,
    })
}
