//! Web-only dependency composition over the single shared workspace mechanism.
pub use auto_generation_core::rust_workspace::*;
use auto_val::AutoResult;
use std::path::{Path, PathBuf};

fn web_source() -> AutoResult<PathBuf> {
    let root = std::env::var_os("AUTO_WEB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let package = root.join("crates/auto-web");
    if !auto_generation_core::source_owner::valid_source_crate(&package, "auto-web") {
        return Err(format!(
            "Web producer source missing or invalid: {}",
            package.display()
        )
        .into());
    }
    auto_generation_core::source_owner::validate_development_dependencies(&package)?;
    Ok(package)
}
pub fn try_ensure_workspace_for(
    project: &Path,
    requirements: WorkspaceRequirements,
) -> AutoResult<PathBuf> {
    // Resolve and validate the selected Web source before touching the prior workspace.
    let package = web_source()?;
    let workspace =
        auto_generation_core::rust_workspace::try_ensure_workspace_for(project, requirements)?;
    let path = workspace.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut document = text.parse::<toml::Value>().map_err(|e| e.to_string())?;
    let deps = document
        .get_mut("workspace")
        .and_then(|v| v.get_mut("dependencies"))
        .and_then(toml::Value::as_table_mut)
        .ok_or("workspace dependencies missing")?;
    let mut dependency = toml::map::Map::new();
    dependency.insert(
        "path".into(),
        toml::Value::String(compute_relative_path(&workspace, &package)),
    );
    dependency.insert("version".into(), toml::Value::String("=0.1.0".into()));
    deps.insert("auto-web".into(), toml::Value::Table(dependency));
    std::fs::write(
        &path,
        toml::to_string(&document).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(workspace)
}

pub fn try_ensure_shared_workspace(project: &Path) -> AutoResult<PathBuf> {
    try_ensure_workspace_for(project, WorkspaceRequirements::Service)
}
pub fn ensure_shared_workspace(project: &Path) -> PathBuf {
    try_ensure_shared_workspace(project).expect("shared Web workspace producer preflight failed")
}
