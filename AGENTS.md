# AutoWeb work rules

Follow the shared PLAN-890 contract in the language main checkout and the explicitly authorized stage. Product modifications, builds and tests run in the existing unique Plan worktree, never the primary checkout. Do not create symlinks/junctions. Do not publish remotely or enter later stages without authorization.

Dependency direction: pure auto-web-contract has no Core/VM/Axum/UI/Man dependency. The framework consumes the public language SDK and restricted shared generation carrier; no dependency on mixed auto-api-gen or UI/Man/Tauri. Never copy VM/AST/Store or maintain duplicate service algorithms.

Use scoped package/real consumer verification for new Web packages; integration with delivered language tm module governance belongs to PLAN-890 T-08. Preserve all original IDs, assertions, platform/ignored/slow conditions. Log/probe evidence stays local; commit Markdown summaries only. Final review and merge gates remain separate from stage commits.

Before any worktree removal run `bash D:/autostack/wt-guard.sh <absolute-worktree>` and require clean. Stage pause retains worktrees and branches.
