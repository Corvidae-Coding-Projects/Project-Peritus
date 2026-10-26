# crates/foundation/peritus-quality-policy/src — navigation

## Responsibility

Exact-revision acceptance policy for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Explicit human approval and waiver observations. |
| [canonical.rs](canonical.rs) | Input-defined canonicality and exact constructor admission. |
| [decision.rs](decision.rs) | Deterministic acceptance decision and typed unmet conditions. |
| [error.rs](error.rs) | Stable failures returned by checked evidence constructors. |
| [evaluator.rs](evaluator.rs) | Total effect-free acceptance evaluation. |
| [evidence.rs](evidence.rs) | Canonical evidence observations and checked evidence aggregate. |
| [finding.rs](finding.rs) | Typed findings and exact-revision dispositions. |
| [gate.rs](gate.rs) | Exact-revision deterministic gate observations. |
| [lib.rs](lib.rs) | Exact-revision acceptance policy for Peritus. |
| [model.rs](model.rs) | Mathematical acceptance and freshness predicates. |
| [ordinal.rs](ordinal.rs) | Checked one-based completion ordinals carried by observations. |
| [proofs.rs](proofs.rs) | Proofs for revision freshness and acceptance completeness. |
| [review.rs](review.rs) | Reviewer identity, independence facts, and review observations. |
| [revision.rs](revision.rs) | Executable refinement of exact revision and acceptance-specification identity equality. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Input-defined canonicality and exact constructor admission. |
| [evaluator/](evaluator/AGENTS.md) | Implementation pieces for `evaluator.rs`: Total effect-free acceptance evaluation. |
| [evidence/](evidence/AGENTS.md) | Implementation pieces for `evidence.rs`: Canonical evidence observations and checked evidence aggregate. |
| [model/](model/AGENTS.md) | Implementation pieces for `model.rs`: Mathematical acceptance and freshness predicates. |
| [proofs/](proofs/AGENTS.md) | Implementation pieces for `proofs.rs`: Proofs for revision freshness and acceptance completeness. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Reviewer identity, independence facts, and review observations. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
