# Runtime and generation boundaries

`auto-web -> auto-generation-core -> auto-lang-core -> auto-web-contract`.
The direct SDK and pure-contract edges share the same Cargo type identities.
Core has no reverse Web implementation edge. Web normal/build/required-dev
closure excludes auto-api-gen, UI, Man and Tauri implementations.

Core owns compiler declarations/snapshots, VM values and conversion, async/native
registries and request-context hooks. The VM/Rc and context guards stay local;
owned Send request/reply/body messages cross to the network thread.

Web owns HTTP transport/dispatch/route matching/scopes, blocking listener grades,
mutable policy/runtime/observation, network/log joins and file/upload executors.
Pure configuration/validation/hash/Host/CORS/proxy rules remain single-source in
`auto-web-contract`. SDK legacy invocation-input functions re-export that type;
new consumers pass typed launch settings explicitly.

Web owns HTTP Rust/TypeScript target generators, service/client generation and
HTTP templates. IPC/Tauri production algorithms remain at their original language
owner. Shared generation is exactly the existing four modules in
`auto-generation-core`; Web's workspace adapter adds its selected source dependency
over that shared mechanism. Source validation precedes workspace replacement.
No VM/AST/Store or shared declaration algorithm is copied.

Generated Rust still uses its original Axum 0.7 track; the VM transport uses Axum
0.8. Their transport-specific objects never cross the neutral SDK contract.
The source-boundary diagnostic fixture retains its synthetic network-tail wrapper
around the relocated source; the package runtime integration test separately
executes the real installed Web crate and CLI.
