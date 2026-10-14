# crates/foundation/peritus-budget/src/transition/allocation — navigation

## Responsibility

Implementation pieces for `allocation.rs`: Child allocation and atomic operation begin transitions.

## Files in this folder

| File | Role |
| --- | --- |
| [begin.rs](begin.rs) | Validation and exact branch selection for operation begin. |
| [begin_apply.rs](begin_apply.rs) | Atomic accounting and reservation insertion for a validated begin. |
| [child.rs](child.rs) | Exact child-account allocation and idempotent replay. |

Owning module: [allocation.rs](../allocation.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
