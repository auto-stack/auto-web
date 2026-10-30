# Current verification boundary

Run in this Web worktree:

- `cargo check --workspace --locked`
- `cargo test -p auto-web --lib <affected module> -- --test-threads=1`
- `cargo test -p auto-web --test http_runtime -- --test-threads=1`
- `cargo test -p auto-web --test http_generation -- --test-threads=1`
- Original HTTP e2e feature stays `test-http-e2e`; ignored/manual conditions stay.

Runtime integration executes the actual installed package over TCP and checks
exact response, control shutdown, owned exit and port release. Generation
integration cold-generates a project, rejects missing selected Web source without
rewriting its old workspace, regenerates, builds the unmodified Rust artifacts,
checks freshness/lock materialization and executes real TCP/close.

The language source-boundary fixture also replays prior audit/session/file/SSE/
upload/isolation controls against the relocated single Web source. Its synthetic
thread-tail wrapper is diagnostic evidence, not the real-package installer test.

Formal tm Web registration, old-ID conservation and four-owner receipts remain
T-08. The final frozen affected module/consumer union remains T-12; the historical
interrupted union has no PASS receipt. Source distribution, fresh/relocated
installed delivery and applications retain their own later gates. See the shared
Plan and handoff for actual commands, outcomes, candidate revisions and hashes.
