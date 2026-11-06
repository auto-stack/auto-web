# HTTP service composition and compatibility

## Public assembly

Direct SDK service consumers add a fixed auto-web dependency and install the
adapter before running the language program:

```rust,ignore
auto_web::install_http_service_host_if_absent();
let launch = auto_web::service_launch(std::sync::Arc::new(config));
auto_lang_core::vm::host_capabilities::http_service::with_http_service_launch(
    Some(launch), || auto_lang_core::run_file("api.at"),
)?;
```

Installation preserves an already selected custom provider; explicit replacement
uses the SDK setter. Ordinary non-service language calls require no Web provider.
Actual service calls without a provider fail before listening; no implicit Core
framework, old runtime fallback or dynamic tool loading is used. Man/UI install
internally at their composition points.

CLI: `auto-web run api.at --config service.json`. Library generation:
`auto_web::generate_api(project, "rust")` or `"vue"`;
`auto_web::backend_generation_is_fresh(project)` checks actual inputs.
HTTP targets: `auto_web::api::{AxumGenerator, TypeScriptGenerator}`.

## Preserved behavior and configuration

Language API injection/return/error/stream/file/upload rules and their explicitly
unsupported cells stay in the [HTTP language specification](../../../auto-lang/docs/specs/stdlib/design/http-server.md).
Supported deployment profiles/Host/CORS/single-proxy trust, budgets, readiness and
controlled shutdown stay in the [deployment contract](../../../auto-lang/docs/specs/stdlib/design/http-service-deployment.md).
The move changes ownership and explicit SDK composition, not those protocols.

Versioned config is strict and captured once. Declared listener/port, profile,
configuration hash and Host policy still apply when legacy environment values
are different, including the public file-entry worker. Original no-config legacy
behavior remains. Development/service loopback defaults and explicit non-loopback
selection retain their old grades; ServerHandle and LegacyStdnet cannot silently
substitute for each other.

VM and generated Rust keep their distinct Axum versions. Network/logger drain,
request scopes, terminal body events, per-service limits, file/upload permits,
subscriptions and cancellation must actually complete; a final response or stop
flag alone is not cleanup evidence. Owned resource completion and service success
are reported separately. Original supported arbitrary Windows path limits,
manual/native requirements and remaining protocol/production gaps do not become
supported merely because source has moved.

Normal JSON replies use the language-owned typed value converter and the active
owner's canonical return declaration. Legacy native string lists are decoded only
under their declared string-list type; ordinary negative integers remain numbers.
This applies to default transport, ServerHandle and LegacyStdnet. Resource/stream
dispatch retains priority; middleware short circuits never borrow the handler type.

## Generated Rust path captures (PLAN-956)

Every declared path capture is extracted, in route segment order, including
multi-capture FileResponse handlers. A single capture uses Path<T>; multiple
captures use a tuple. Exact segment names determine path/query/body membership:
`:ids` does not consume a parameter named `id`; `*resource` is a path capture.
Declaration order may differ from route order. Reader's `/api/reader/media/:bid/:res`
is a real compiled two-capture file endpoint, exercised with original PNG bytes.

### Generated Rust DB sibling imports (PLAN-990)

For standard src/back projects, standalone Rust generation supplies db.at and companion modules with the project src/back source directory used by assembly proof collection. Sibling import declarations provide record field types to the existing language String storage and borrow rules. The db module keeps its existing post-processing and handler wiring; source-region and per-function ownership controls, legacy standalone transpiler entry points, import fallback rules and HTTP protocols remain unchanged.
