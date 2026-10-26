# crates/foundation/peritus-security-policy/src — navigation

## Responsibility

H0 security-readiness policy for an exact integrated candidate.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Exact integrated-candidate and revision binding. |
| [catalog.rs](catalog.rs) | Closed authoritative H0 requirement and evidence identities. |
| [decision.rs](decision.rs) | Private-construction H0 decision and deterministic unmet conditions. |
| [error.rs](error.rs) | Typed canonical-evidence construction failures. |
| [evaluator.rs](evaluator.rs) | Total effect-free security-readiness evaluation. |
| [evidence.rs](evidence.rs) | Canonical aggregate supplied to the H0 policy evaluator. |
| [lib.rs](lib.rs) | H0 security-readiness policy for an exact integrated candidate. |
| [model.rs](model.rs) | Mathematical H0 readiness aggregation. |
| [observation.rs](observation.rs) | Exact-candidate observations for requirements, criteria, inventories, and artifacts. |
| [proofs.rs](proofs.rs) | Structural H0 readiness proof obligations. |
| [qualification_admission.rs](qualification_admission.rs) | Finite admission for the production H0 qualification boundary. |
| [review.rs](review.rs) | Independent external review and finding lifecycle observations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [evaluator/](evaluator/AGENTS.md) | Implementation pieces for `evaluator.rs`: Total effect-free security-readiness evaluation. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Independent external review and finding lifecycle observations. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
