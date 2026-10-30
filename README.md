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
Man/UI composition is wired through the public Web exports. The language
governance catalog registers `web-unit`, `web-http`, and `web-generation` as
separate recipes with the original IDs/conditions preserved across their move.
Independent source distribution/recovery (T-09), application acceptance and final
review/landing remain pending. Local package checks do not certify those gates.

Run the independent Web tests in this repository:

```text
cargo nextest run -p auto-web-contract -p auto-web --lib --features auto-web/test-http-e2e
cargo nextest run -p auto-web --test http_runtime --test http_generation
```

The Web nextest configuration keeps HTTP processes serial and preserves the
original full-tier deadlines. Original ignored tests remain explicitly ignored.
`crates/auto-web/tests/fixtures/885-back` retains the original generator fixture
bytes; language VM/standard-library boundary fixtures stay with their consumers.
