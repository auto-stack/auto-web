# AutoWeb

Web service framework extracted under PLAN-890. Product code is in the existing
`lang-890/auto-web` worktree until the Plan's final delivery gates pass.

Explicit SDK composition:

```rust,ignore
auto_web::install_http_service_host_if_absent();
let launch = auto_web::service_launch(std::sync::Arc::new(config));
auto_lang_core::vm::host_capabilities::http_service::with_http_service_launch(
    Some(launch), || auto_lang_core::run_file("api.at"),
)?;
```

`WebHttpServiceHost` implements the language SDK's local `HttpServiceHost` port.
The installer never replaces a custom provider. Explicit replacement is the
SDK's existing setter. Ordinary language calls need no Web host; service calls
without an installed host fail before binding. No Core runtime fallback exists.

Local CLI: `cargo run -p auto-web -- run api.at --config service.json`.
Library generation: `auto_web::generate_api(project, "rust")` or `"vue"`;
`backend_generation_is_fresh(project)` checks real source/assembly/lock inputs.
HTTP target generators live at `auto_web::api::{AxumGenerator, TypeScriptGenerator}`.
Tauri IPC remains in `auto-api-gen`; its legacy dispatcher delegates HTTP here.

The central contract is the language main checkout's
`docs/plans/890-auto-web-independent-repository-delivery.md`.
Man/UI composition (T-07), formal test-ID/four-owner governance (T-08), independent
source distribution/recovery (T-09), application acceptance and final review/
landing remain separate stages. Local package checks do not certify those gates.
