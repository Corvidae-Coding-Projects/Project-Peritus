# crates/runtime/peritus-workspace/src/git_mutation_recovery — navigation

## Responsibility

Implementation pieces for `git_mutation_recovery.rs`: Durable replay of completed candidate and rollback actions.

## Files in this folder

| File | Role |
| --- | --- |
| [planned.rs](planned.rs) | Finish a retained, preplanned Git effect before publishing its exact receipt. |

Owning module: [git_mutation_recovery.rs](../git_mutation_recovery.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
