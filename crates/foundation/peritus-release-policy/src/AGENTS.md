# crates/foundation/peritus-release-policy/src — navigation

## Responsibility

H4 production release policy for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [candidate.rs](candidate.rs) | Exact release-candidate, platform, toolchain, profile, and schema identities. |
| [catalog.rs](catalog.rs) | Closed production-acceptance and evidence catalogs. |
| [decision.rs](decision.rs) | Canonical assessments, diagnostics, and non-authorizing release verdict. |
| [error.rs](error.rs) | Typed checked-construction failures. |
| [evaluator.rs](evaluator.rs) | Total deterministic H4 release evaluation. |
| [evidence.rs](evidence.rs) | Bounded observations for canonical H4 release evidence. |
| [identity.rs](identity.rs) | Nominal release-policy identities. |
| [lib.rs](lib.rs) | H4 production release policy for Peritus. |
| [model.rs](model.rs) | Mathematical evidence freshness and permutation predicates. |
| [prelude.rs](prelude.rs) | Common imports for callers assembling and evaluating H4 evidence. |
| [qualification.rs](qualification.rs) | Signed H0-H3 qualification observations used as verified policy inputs. |
| [qualification_admission.rs](qualification_admission.rs) | Finite admission for the production H4 qualification boundary. |
| [review.rs](review.rs) | Independent review, finding, and waiver observations. |
| [validation.rs](validation.rs) | Shared checked-construction predicates for release-policy values. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [candidate/](candidate/AGENTS.md) | Implementation pieces for `candidate.rs`: Exact release-candidate, platform, toolchain, profile, and schema identities. |
| [decision/](decision/AGENTS.md) | Implementation pieces for `decision.rs`: Canonical assessments, diagnostics, and non-authorizing release verdict. |
| [evaluator/](evaluator/AGENTS.md) | Implementation pieces for `evaluator.rs`: Total deterministic H4 release evaluation. |
| [evidence/](evidence/AGENTS.md) | Implementation pieces for `evidence.rs`: Bounded observations for canonical H4 release evidence. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Independent review, finding, and waiver observations. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
