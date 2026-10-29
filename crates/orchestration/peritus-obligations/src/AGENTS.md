# crates/orchestration/peritus-obligations/src — navigation

## Responsibility

Public requirement obligations, qualification evidence, and failure ownership.

## Files in this folder

| File | Role |
| --- | --- |
| [browser.rs](browser.rs) | Browser-semantics requirements and standards-implementation evidence. |
| [canonical.rs](canonical.rs) | Canonical digest preimage for one exact requirement ledger. |
| [condition.rs](condition.rs) | Public-condition observations controlling conditional obligations. |
| [error.rs](error.rs) | Stable checked-construction and qualification failures. |
| [evidence.rs](evidence.rs) | Candidate- and ledger-bound typed obligation evidence. |
| [failure.rs](failure.rs) | Closed failure ownership and legal next transitions. |
| [identity.rs](identity.rs) | Stable typed identities used only by the obligation domain. |
| [ledger.rs](ledger.rs) | Deterministic extraction of typed requirements from exact public source spans. |
| [lib.rs](lib.rs) | Public requirement obligations, qualification evidence, and failure ownership. |
| [lifecycle.rs](lifecycle.rs) | Real public lifecycle-ingress requirements and observations. |
| [limits.rs](limits.rs) | Explicit bounds for untrusted public clauses and extracted obligation collections. |
| [matching.rs](matching.rs) | Exact byte identity comparisons shared by verified obligation kernels. |
| [order.rs](order.rs) | Exact lexicographic ordering of canonical identity bytes. |
| [path.rs](path.rs) | Exact path mentions with explicit acceptance roles. |
| [performance.rs](performance.rs) | Same-workload performance requirements and measured evidence. |
| [provenance.rs](provenance.rs) | Exact public task content and clause provenance. |
| [qualification.rs](qualification.rs) | Fail-closed qualification of exact public obligations against current typed evidence. |
| [requirement.rs](requirement.rs) | Typed requirement entries extracted from exact public clauses. |
| [schema.rs](schema.rs) | Direction-specific request and response schema obligations. |
| [schema_coverage.rs](schema_coverage.rs) | Exact linear subset matching for canonical schema field identities. |
| [schema_model.rs](schema_model.rs) | Exact logical views of directional schema requirements and observations. |
| [schema_validation.rs](schema_validation.rs) | Exact validation of canonical required and observed schema fields. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `qualification_allowed`, `conditional_obligation_active`, `alternative_group_complete`, `failure_authorizes_fixer`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Canonical digest preimage for one exact requirement ledger. |
| [evidence/](evidence/AGENTS.md) | Implementation pieces for `evidence.rs`: Candidate- and ledger-bound typed obligation evidence. |
| [ledger/](ledger/AGENTS.md) | Implementation pieces for `ledger.rs`: Deterministic extraction of typed requirements from exact public source spans. |
| [path/](path/AGENTS.md) | Implementation pieces for `path.rs`: Exact path mentions with explicit acceptance roles. |
| [performance/](performance/AGENTS.md) | Implementation pieces for `performance.rs`: Same-workload performance requirements and measured evidence. |
| [qualification/](qualification/AGENTS.md) | Implementation pieces for `qualification.rs`: Fail-closed qualification of exact public obligations against current typed evidence. |
| [requirement/](requirement/AGENTS.md) | Implementation pieces for `requirement.rs`: Typed requirement entries extracted from exact public clauses. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
