# Foundation provenance (PLAN-890 T-03/T-04)

Pure configuration/policy source moved byte-for-byte from language `crates/auto-lang-core/src/http_service_config/contract.rs`, code baseline `f56211c12`, synchronized branch `29ed9d7bd`.
Original source SHA256: `96ea1f0ebedd09ece973c67ad51da79f3a9d68b03904101b9e8902fcf4c8d09c`.
Original MIT license is retained at repository root. No remote import/publication was performed.

At the T-03/T-04 checkpoint the framework exposed only the pure contract and real invocation-launch construction. It did not yet install or own the HTTP runtime, generator, CLI/UI consumption, distribution or test governance. The actual T-05/T-06 transfer is recorded below; consumer/distribution/governance gates remain later stages. See the shared main Plan and stage handoff for actual candidate pins.


## Runtime/generator transfer (T-05/T-06)

Source baseline: language `1a253950810de67f16f487c7eb7a2745f1061bb0`
(code), handoff `e45399a633a1dcf74f70943cf2d2c663791589a6`.
MIT ownership/license notices and historical Plan comments are retained.

| Original language source | Web destination / retained language portion |
|---|---|
| `auto-lang-core/src/http_{file,upload}_service.rs` | `auto-web/src/http_{file,upload}_service.rs`; VM native registries remain Core |
| `auto-lang-core/src/http_service_{config,observability}.rs` | Web runtime/policy/observation; pure config remains contract and invocation input remains Core |
| `auto-lang-core/src/vm/ffi/http_{server,transport}.rs` and server children/handle | Web same filenames; declaration snapshots/JSON conversion/resource hooks remain one Core source |
| `auto-api-gen/src/api_gen.rs` | Web HTTP generator/tests/templates; IPC producer and its three tests remain original owner |
| `auto-lang-core/src/api/targets/{axum,typescript}.rs` | Web `api_targets/`; extractor/types/contract/trait and Tauri stay Core |
| `auto-lang-core/src/api/integration_tests.rs` | Web cross-target contract tests still call the original Core Tauri generator |

Original test bodies/assertions/ignore/feature conditions are preserved across
these owners. Formal old-ID/new-owner execution registration is T-08, not a
claim inferred from the source move. Source inventory/hashes and candidate pins
are in the central stage handoff. No remote repository or publication was made.
