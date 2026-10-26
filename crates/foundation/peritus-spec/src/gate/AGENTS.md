# crates/foundation/peritus-spec/src/gate — navigation

## Responsibility

Implementation pieces for `gate.rs`: Checked deterministic gate declarations and dependency graph.

## Files in this folder

| File | Role |
| --- | --- |
| [query.rs](query.rs) | Read-only graph queries over the validated stored execution order. |

Owning module: [gate.rs](../gate.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
