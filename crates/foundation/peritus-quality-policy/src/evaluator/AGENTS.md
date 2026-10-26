# crates/foundation/peritus-quality-policy/src/evaluator — navigation

## Responsibility

Implementation pieces for `evaluator.rs`: Total effect-free acceptance evaluation.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Blocker, waiver, and final human-approval evaluation. |
| [freshness.rs](freshness.rs) | INV-003 executable freshness checks. |
| [gates.rs](gates.rs) | Required gate evaluation. |
| [requirements.rs](requirements.rs) | Required artifact completeness checks. |
| [reviews.rs](reviews.rs) | Review quorum, category, and independence evaluation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [authority/](authority/AGENTS.md) | Implementation pieces for `authority.rs`: Blocker, waiver, and final human-approval evaluation. |
| [reviews/](reviews/AGENTS.md) | Implementation pieces for `reviews.rs`: Review quorum, category, and independence evaluation. |

Owning module: [evaluator.rs](../evaluator.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
