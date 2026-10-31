# Web architecture and ownership

> Source: PLAN-890; public assembly in crates/auto-web/src/{lib,host}.rs.

## One implementation per responsibility

`auto-web -> auto-generation-core -> auto-lang-core -> auto-web-contract`.
Core has no reverse Web implementation edge. Web normal/build/required independent
dev closure excludes auto-api-gen, UI, Man and Tauri implementations. The pure
contract has no Core/VM/Axum implementation. Cargo resolves one physical source
and type identity; a same-named foreign SDK/contract source must reject.

Core owns compilation declarations and frozen per-program metadata, values/JSON
conversion, native/async registries and resource/context hooks. The process-wide
provider implements `HttpServiceHost: Send + Sync`; its `HttpServiceFuture` and
`Rc<AutoVM>` remain local/!Send. Owned request/reply/body/cancellation messages
cross to the network owner; VM references do not. No VM/AST/Store copy, integer
pointer transport or framework-specific object crosses the neutral contract.

Web owns transport/route/binding/dispatch/scope, blocking listener grades,
mutable service policy/runtime/observations, network/logger joins, file/upload
execution. Pure config/validation/hash/Host/CORS/proxy settings are single-source
in auto-web-contract; SDK compatibility paths re-export that type.

Compilation metadata freezes before execution, separately from invocation launch.
Each active owner keeps its own snapshots. Entry workers, including file execution,
capture typed launch before spawning; every future poll restores its own context.
Explicit absent launch stays absent. The global compatibility input cannot replace
an active service snapshot. Details: [language host port](../../../auto-lang/docs/specs/language-sdk/http-host-port.md).

## Generation and shared carriers

Web owns `generate_api`, `backend_generation_is_fresh`, HTTP Axum/TypeScript targets,
service/client generation and HTTP templates. IPC/Tauri and non-HTTP adapters keep
their language owner. Exactly four existing shared mechanisms are single-source
in auto-generation-core: rust_workspace/shared_generation/sidecar/source_owner.
Do not copy declaration/type lifting, locking, freshness or source-validation
algorithms. Selected source/SDK validation happens before workspace replacement.
Inherited [PLAN-885 runtime/import/type/borrow rules](../../../auto-lang/docs/specs/auto-api-gen/project.md)
stay in force. Renderer-neutral photo_service and its original ten tests belong
to auto-media-core; UI's old path is only a re-export.

VM transport uses Axum0.8; generated Rust retains Axum0.7. These are distinct
transport tracks with their original supported contracts, not a unified server.
This extraction introduces no TLS/auth/telemetry product, new platform promise
or independent alternative installer algorithm.
