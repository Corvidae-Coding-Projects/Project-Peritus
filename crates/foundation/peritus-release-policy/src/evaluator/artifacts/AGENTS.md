# crates/foundation/peritus-release-policy/src/evaluator/artifacts — navigation

## Responsibility

Implementation pieces for `artifacts.rs`: Exact reduction of supplied release-artifact observations and their criteria.

## Files in this folder

| File | Role |
| --- | --- |
| [assessment.rs](assessment.rs) | One exact release-evidence requirement assessment. |
| [criteria.rs](criteria.rs) | Canonical reduction of artifact assessments into release criteria. |

Owning module: [artifacts.rs](../artifacts.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
