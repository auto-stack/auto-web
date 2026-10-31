# Source-bound testing and actual execution

Run in the authoritative owner tree. Preserve original IDs, assertions,
features/ignored/platform/manual/milestone conditions and formal samples.
HTTP processes remain serial under the existing nextest isolation/resources.

| Surface | Entry |
|---|---|
| Library and original e2e | cargo nextest run -p auto-web-contract -p auto-web --lib --features auto-web/test-http-e2e |
| Public runtime | cargo nextest run -p auto-web --test http_runtime |
| Real generation | cargo nextest run -p auto-web --test http_generation |
| Module/consumer union | language cargo tm explicit IDs/changed paths, including web-unit/web-http/web-generation |

Runtime tests use the actual installed Web crate/CLI over TCP: exact response,
different config/environment ports, loopback/profile/config hash, Host rejection,
normal shutdown/exit and port release. Generation tests cold-generate and build
unmodified Rust artifacts, check source rejection preserves the old workspace,
freshness/locks, actual TCP and controlled exit. Diagnostic source-boundary
wrappers do not replace real installed-package/runtime evidence.

Original migration/resource contracts live in the language scripts/test-governance:
web-test-migrations.json, media-test-migrations.json, web-resource-ownership.json
and web-historical-obligations.json. The original885 generator fixture is retained
at crates/auto-web/tests/fixtures/885-back with unchanged bytes.

The four-owner gate freezes language/Web/UI/Man plus the fixed Down dependency.
Actual complete language modules/consumer closure execute once; Web/media receipt
legs come from their original atomic outputs. UI/Man retain their own commands;
ui_gen remains single-process. Source, selection, command, output/hash, exact
passed IDs and internal case/child comparisons are checked at consumption.
Producer and consumer share the original source-baseline/floor/retirement rules;
consistent omission is invalid. Migration credit requires passed target IDs.
Original libtest ignores derive from the same-source ui-full nextest flags and
are exact N/A exclusions, not passed bodies. Purpose advice changes no eligibility.

List-only/filtered/partial/duplicate/zero/unexecuted/stale-input or missing command
proof cannot certify delivery. Required real distribution/current-cohort checks
are separate. [Central Plan](../../../auto-lang/docs/plans/archive/890-auto-web-independent-repository-delivery.md)
contains actual first failures, reuse boundaries and final gate receipts; a scoped
pass or historical catalog is not full certification. No AG milestone suite is
added to this extraction's merge obligation.
