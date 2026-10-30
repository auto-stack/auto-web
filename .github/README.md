# CI integration contract

Foundation validation is `cargo check --workspace --locked` from this repository.
Provide the exact candidate language SDK and auto-generation-core source as the sibling `auto-lang` before execution. Record both Git commits, lock hashes and package closure. Required closure excludes UI/Man/Tauri and the mixed API facade.

No remote repository, CI runner or publishing channel is configured or certified in the local bootstrap. Workflow wiring must consume formal distribution/source pins; it must not guess repository URLs or fetch an unreviewed default branch. PLAN-890 T-09 owns independent distribution/CI admission; T-08 owns migrated test/module registration.
