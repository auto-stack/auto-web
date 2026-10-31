# Independent delivery, source selection and recovery

> PLAN-890 local independent delivery. Final combined T-12 gates: pending at preparation.

## Package and source boundaries

The version0.1.0 source/Windows payload carries twenty actual SDK packages (including the PLAN-896 auto-tool-execution dependency),
complete stdlib, required fixtures/licenses and the real owner tool artifact.
Man owns the generic prepare/verify/swap/rollback transaction; Web embeds a
byte-exact, Git-provenanced immutable artifact, never a separately editable fork.

Use `python scripts/web-install.py source|verify-source|install|verify` and
`scripts/web-repository-distribution.py` according to their public --help. Validate source receipt,
owned file inventory/hash, package/producer versions and required resources before
staging or replacing a workspace. An explicit selected or adjacent managed SDK
failure cannot borrow ambient sources. Old valid user content/installation survives
prepare/verify/swap failure; successful upgrade is followed by actual read/execute.
Service generation accepts Web minimal SDK and valid Man Core/Full; UI requires
Full. The old architecture recovery claim is its genuine Core shape, not old UI/Full.

## Evidence and limits

Earlier verified work evidence: actual independent SDK library217passed/two original
ignored; fresh/locked/relocated real VM and generated Rust consumers; historical
45 distribution checks and14 legacy Core recovery checks, each bound to its original
producer/input. Rebuilt configured CLI and fixed Blog source b42a660 were independently
verified: Vue15/15, native22/22; cold Rust build preceded those runtime profiles.
These are scoped inputs, not a claim of new aggregate current-cohort gates.

Historical Windows long-path failures, first raw reds, original ignores, controller
cleanup limits and warnings remain recorded. Source identity/hash establishes
provenance, not permanent raw artifact availability. Raw evidence is local under
the user retention/budget policy; durable summaries live in the central Plan.

Final gate status and actual landed cohort are recorded in the
[central delivery receipt](../../../auto-lang/docs/plans/archive/890-auto-web-independent-repository-delivery.md).
Before landing, execute the frozen explicit affected `cargo t` fixed groups and necessary consumers/integration,
real four-owner gate and current-cohort release/history checks. After landing,
verify canonical/ledger/index readback, archive and every guarded tree cleanup.
A final review pass does not waive a failed gate. Due periodic regression belongs
to the language primary checkout. Remote push/registry publication was not authorized.
