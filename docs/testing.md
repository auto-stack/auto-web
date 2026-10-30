# Current verification boundary

Run in this Web worktree:

- `cargo check --workspace --locked`
- `cargo nextest run -p auto-web --lib -E 'test(<affected module>)'`
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

The language tm catalog registers `web-unit` (full feature-enabled library),
`web-http` (real runtime integration) and `web-generation` (real generation).
Tests keep isolated nextest processes and serial HTTP execution; a serial
libtest process does not isolate each test's global-limit initialization.
The source/ID/condition and 17-resource contracts live in the language
`scripts/test-governance/web-*.json` files. The original 885 generator fixture
has moved here at `crates/auto-web/tests/fixtures/885-back` with unchanged bytes.

The final frozen affected module/consumer union remains T-12; the historical
interrupted union has no PASS receipt. Source distribution, fresh/relocated
installed delivery and applications retain their own later gates. See the shared
Plan and handoff for actual commands, outcomes, candidate revisions and hashes.
