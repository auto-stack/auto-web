# Foundation boundaries

`auto-web -> auto-generation-core -> auto-lang-core -> auto-web-contract`.
`auto-web -> auto-lang-core` and `auto-web -> auto-web-contract` are direct dependencies.

Pure contract: strict config/validation/hash/Host/CORS/proxy policy, no VM/runtime.
Language: metadata, VM, owned messages, launch/request-context/resource bridge.
Restricted generation carrier: exactly four existing shared modules, no HTTP/UI policy.
Framework: currently contract re-export plus invocation-launch construction. Service runtime/HTTP generation transfer in T-05/T-06; bootstrap is not runtime delivery.

Cross-repository authority remains the shared PLAN-890 main Plan in auto-lang. Runtime/private compiler objects never cross into the pure contract.
