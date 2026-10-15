# crates/orchestration/peritus-gates/src/durability — navigation

## Responsibility

Implementation pieces for `durability.rs`: Atomic C0 persistence and checked replay loading for one D1 aggregate.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Cross-record semantic binding checks before C0 planning. |
| [lifecycle.rs](lifecycle.rs) | Plan-independent persistence for the two D1 lifecycle-only commands. |

Owning module: [durability.rs](../durability.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
