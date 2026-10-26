# crates/foundation/peritus-budget/src/reachability/rejections — navigation

## Responsibility

Implementation pieces for `rejections.rs`: Branch-ordered typed rejection relation for total reducer outcomes.

## Files in this folder

| File | Role |
| --- | --- |
| [allocation.rs](allocation.rs) | Branch-ordered allocation and begin rejection predicates. |
| [reservation.rs](reservation.rs) | Branch-ordered reservation reconciliation rejection predicates. |

Owning module: [rejections.rs](../rejections.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
