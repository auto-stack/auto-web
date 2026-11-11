# Generated service-member delivery (export-only bundles)

> PLAN-1006. Public entry: `auto service-export --dir <project> --delivery-root <bundle>
> [--resource <rel>]...` (Man thin dispatch) over the `auto_web::delivery::export_rust_service`
> library contract. Status: implemented and library-verified; external driver/real-product
> receipts live in the central Plan 1006 record.

## What a bundle is

A delivery bundle is a relocatable, self-contained build root for ONE generated Rust
service member of a normal Auto project:

```
<bundle>/Cargo.toml           workspace: members=["app/<member>"], exclude=["sdk"],
                              dependency anchors into sdk/crates/*, C/full profile block
<bundle>/Cargo.lock           generated inside the staged bundle (cargo generate-lockfile)
<bundle>/.cargo/config.toml   local target dir + Windows MSVC stack reserve
<bundle>/app/<member>/        the freshly generated member crate (receipts sanitized)
<bundle>/sdk/crates/<name>/   SDK closure crates, cross-repo anchors rewritten to ../<name>
<bundle>/sdk/experimental/ace/crates/semantics-contract/   same-shape vendor tree
<bundle>/sdk/vendor/autodown-test-parser/                  same-shape vendor tree
<bundle>/sdk/crates/auto-lang/test/cases/  facade test-cases data tree (compile-time
                              include inputs of auto-lang-core; the facade crate itself
                              is never shipped as a package)
<bundle>/sdk/stdlib/...       compile-time root inputs (contracts tomls, assembly
                              providers, image stdlib files) mirrored from the producer
<bundle>/sdk/Cargo.toml       projected producer workspace (closure members only,
                              [workspace.package] pinned) + producer Cargo.lock copy
<bundle>/sdk/licenses/...     auto-lang / auto-web licenses
<bundle>/resources/...        explicit declared project files (none declared = absent)
<bundle>/delivery-manifest.json  identity (schema web-member-delivery/1)
```

Consumption contract: from the bundle root, `cargo build --locked` with generation
roots scrubbed (`AUTO_LANG_ROOT`/`AUTO_WEB_ROOT`/`AUTO_RUST_WORKSPACE`/... absent or
pointing away) builds and runs the member; the bundle never anchors back to a producer
checkout (verified by a whole-bundle dev-path leak scan at export time).

## Export semantics

- Strict-only admission: no back contract, strict-parse failure, lenient scaffold
  placeholders or endpoint-free (VM-only) sources are named errors; an empty bundle is
  never produced. `AUTO_API_LENIENT=1` does not downgrade export.
- Generation keeps its normal development behavior: export first runs the ordinary
  `generate_api(project,"rust")` (semantic root and landing position unchanged, receipt
  freshness enforced), then harvests the fresh member crate into the staged bundle.
  The generated Rust workspace of the project is never modified by delivery.
- Source selection follows the established explicit env order
  (AUTO_DELIVERY_SOURCE_ROOT/AUTO_LANG_ROOT/AUTO_LANG_CRATE/AUTO_WEB_ROOT → adjacent
  installed SDK → discovery → compiled-in). Export never mutates process env. Mixed
  roots (web crate and language producer from different groups) fail closed with a
  named error instead of silently merging.
- Closure: enumerated from the member's four direct SDK crates via the neutral
  `local_build_closure` (normal/build path deps only; identity-, version- and
  cycle-checked). Forbidden: `auto-lang` (facade), `auto-api-gen`, `tauri`, any
  `auto-ui*`/`auto-man*`. Non-`crates/` layouts are allowed only for the whitelisted
  vendor trees above.
- Resources are explicit-only (`--resource`, project-internal plain files). Escapes
  (`..`/absolute), junctions/symlinks, missing files and excluded trees (.git/target/
  node_modules) are named errors; no disk scanning, no private data inclusion.
- Debug policy (PLAN-1003): resolved before any delivered byte; the bundle records the
  static label (`c` default / `full` via AUTO_GENERATED_DEBUG_POLICY) and carries
  `managed_profile_toml`; generation-time user partial overrides are not inherited.

## Transaction and failure preservation

Staging → verify → commit through the neutral `auto_generation_core::commit`
transaction (owner: auto-lang; see the language lifecycle spec). A non-empty delivery
root without a valid `delivery-manifest.json` is refused untouched. A prior bundle is
backed up (rename, bytes preserved) and restored byte-for-byte on any failure; a
payload that already swapped in is preserved in a `.auto-failed-*` sibling; same-pid
staging leftovers are swept, foreign staging directories are preserved and reported.
Injected failure points (`AUTO_EXPORT_FAIL_POINT=prepare|verify|swap`) exist for
transaction tests and recovery drills only.

## Identity

`delivery-manifest.json` records schema/version, producer HEADs (auto-lang, auto-web),
member name, the full source closure (packages/vendor trees/root inputs), static debug
policy, lock sha256, exporter identity, and a per-file {bytes, sha256} map with
fingerprint. The bundle carries no wall-clock timestamps: member receipts are
sanitized in-bundle (development build-context anchors AND `generated_at` /
`created_at_ms` wall-clock fields dropped; semantic fields kept), so repeated exports
of unchanged inputs (same project, sources, lock, registry resolution, resources) are
byte-stable — pinned by a repeat-export identity test. Observable identity changes
only when real inputs change (sources, lock, tools, config, resources). Producer
revisions come from `git rev-parse HEAD`; for git-less pinned source copies (driver
drills) a `.source-revision` marker holding the 40-hex revision the copy was taken
from is accepted as provenance (content identity is still pinned by the per-file
hash map; an invalid marker is a named error).

## Recovery (RC-WEB-01, first-version controlled pairs)

Bundles and user data are separated: runtime state lives in an explicit external data
root (fixture contract: `MEMBER_DELIVERY_DATA`, required; real products: chat
`CHAT_DATA_DIR` (whole directory), database per-request paths — never HOME/APPDATA
fallbacks). First-version A/B recovery is a controlled same-schema pair produced by
the same exporter from versioned sources: A writes state → stop + back up data → B
prepared/verified/swapped in → B continues reading the same schema → any B
prepare/verify/commit failure keeps A and its data → restore A + data backup and read
back independently. No historical-release or cross-schema upgrade is claimed.

## Non-goals

No shared workspace, no installer/SDK-distribution replacement (web-install.py keeps
its own contract), no cross-platform binary packaging, no cold-registry guarantee,
no user-data migration, no re-generation inside the bundle.
