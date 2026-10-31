# AutoWeb

Independent Web service framework, version0.1.0. Runtime, HTTP generation,
resources/tests and independent delivery have one owner here.

Start with [canonical project](docs/specs/project.md), [architecture](docs/specs/architecture.md),
[HTTP/SDK composition](docs/specs/http-contracts.md), [testing](docs/specs/testing.md)
and [source/Windows delivery and recovery](docs/specs/delivery.md).

```rust,ignore
auto_web::install_http_service_host_if_absent();
let launch = auto_web::service_launch(std::sync::Arc::new(config));
auto_lang_core::vm::host_capabilities::http_service::with_http_service_launch(
    Some(launch), || auto_lang_core::run_file("api.at"),
)?;
```

CLI: `cargo run -p auto-web -- run api.at --config service.json`.
Generation: `auto_web::generate_api(project, "rust")` / `"vue"`;
`backend_generation_is_fresh(project)` checks actual source/assembly/lock inputs.
Installation preserves custom providers. Ordinary language execution needs no Web;
service calls without a host reject before listening. Man/UI compose internally.

The [central Plan](../auto-lang/docs/plans/archive/890-auto-web-independent-repository-delivery.md)
separates independent acceptance from actual final gates and local landings.
Local Git delivery does not mean remote push/registry release. Provenance remains
in [docs/source-provenance.md](docs/source-provenance.md).
