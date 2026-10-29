# crates/orchestration/peritus-obligations/src/qualification — navigation

## Responsibility

Implementation pieces for `qualification.rs`: Fail-closed qualification of exact public obligations against current typed evidence.

## Files in this folder

| File | Role |
| --- | --- |
| [evaluation.rs](evaluation.rs) | Ordinary and alternative qualification traversal. |
| [validation.rs](validation.rs) | Condition and evidence admission in production error order. |
| [verdict.rs](verdict.rs) | Lookups, required-path checks, and typed evidence verdicts. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [evaluation/](evaluation/AGENTS.md) | Implementation pieces for `evaluation.rs`: Ordinary and alternative qualification traversal. |

Owning module: [qualification.rs](../qualification.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
