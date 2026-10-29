# crates/foundation/peritus-release-policy/src/evaluator — navigation

## Responsibility

Implementation pieces for `evaluator.rs`: Total deterministic H4 release evaluation.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | Exact input admission predicates for release evidence. |
| [artifacts.rs](artifacts.rs) | Exact reduction of supplied release-artifact observations and their criteria. |
| [diagnostics.rs](diagnostics.rs) | Canonical diagnostic emission from completed assessments. |
| [findings.rs](findings.rs) | Finding and waiver policy reduction. |
| [qualifications.rs](qualifications.rs) | H0-H3 qualification reduction. |
| [reviews.rs](reviews.rs) | Independent-review reduction over the supplied observations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [artifacts/](artifacts/AGENTS.md) | Implementation pieces for `artifacts.rs`: Exact reduction of supplied release-artifact observations and their criteria. |
| [diagnostics/](diagnostics/AGENTS.md) | Implementation pieces for `diagnostics.rs`: Canonical diagnostic emission from completed assessments. |
| [findings/](findings/AGENTS.md) | Implementation pieces for `findings.rs`: Finding and waiver policy reduction. |

Owning module: [evaluator.rs](../evaluator.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
