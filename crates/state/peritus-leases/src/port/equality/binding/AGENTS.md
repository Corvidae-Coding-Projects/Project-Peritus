# crates/state/peritus-leases/src/port/equality/binding — navigation

## Responsibility

Implementation pieces for `binding.rs`: Exact equality for the closed lease command-binding family.

## Files in this folder

| File | Role |
| --- | --- |
| [commands.rs](commands.rs) | Exact equality for the copyable lease command families. |
| [use_projection.rs](use_projection.rs) | Exact equality for complete policy-use command projections. |

Owning module: [binding.rs](../binding.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
