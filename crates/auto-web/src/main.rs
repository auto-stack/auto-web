//! Explicit local Web host composition.
fn run() -> auto_lang_core::AutoResult<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(args.len() == 2 || args.len() == 4 && args[2] == "--config") || args[0] != "run" {
        return Err("usage: auto-web run <source.at> [--config <service.json>]".into());
    }
    let launch = if args.len() == 4 {
        let config = auto_web::contract::resolve_service_config(
            &std::fs::read_to_string(&args[3]).map_err(|e| e.to_string())?,
            None,
        )
        .map_err(|e| e.to_string())?;
        Some(auto_web::service_launch(std::sync::Arc::new(config)))
    } else {
        None
    };
    auto_web::install_http_service_host_if_absent();
    auto_lang_core::vm::host_capabilities::http_service::with_http_service_launch(launch, || {
        auto_lang_core::run_file(&args[1])
    })?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
