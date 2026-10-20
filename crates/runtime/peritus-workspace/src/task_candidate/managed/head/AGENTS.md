# crates/runtime/peritus-workspace/src/task_candidate/managed/head — navigation

## Responsibility

Implementation pieces for `head.rs`: Restore nested HEAD without rewriting or deleting any existing branch.

## Files in this folder

| File | Role |
| --- | --- |
| [prepared.rs](prepared.rs) | Keep Git HEAD locks alive from preflight through source and index restoration. |

Owning module: [head.rs](../head.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
