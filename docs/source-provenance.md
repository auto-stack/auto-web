# Foundation provenance (PLAN-890 T-03/T-04)

Pure configuration/policy source moved byte-for-byte from language `crates/auto-lang-core/src/http_service_config/contract.rs`, code baseline `f56211c12`, synchronized branch `29ed9d7bd`.
Original source SHA256: `96ea1f0ebedd09ece973c67ad51da79f3a9d68b03904101b9e8902fcf4c8d09c`.
Original MIT license is retained at repository root. No remote import/publication was performed.

The framework currently exposes only the pure contract and real invocation-launch construction. It does not yet install or own the HTTP runtime, generator, CLI/UI consumption, distribution or test governance. Those remain explicit later PLAN-890 stages; existing Core runtime bridges must exit in T-05. See the shared main Plan and stage handoff for actual candidate pins.
