# crates/state/peritus-migrations/src/engine — navigation

## Responsibility

Implementation pieces for `engine.rs`: Single-owner migration engine and forward-only apply state machine.

## Files in this folder

| File | Role |
| --- | --- |
| [apply.rs](apply.rs) | Exclusive migration transaction application and commit classification. |
| [restart.rs](restart.rs) | Restart reconciliation and explicit backup restoration. |

Owning module: [engine.rs](../engine.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
