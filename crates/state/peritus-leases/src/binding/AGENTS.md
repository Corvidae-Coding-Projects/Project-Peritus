# crates/state/peritus-leases/src/binding — navigation

## Responsibility

Implementation pieces for `binding.rs`: Exact unprivileged command projections carried through durable lease plans.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Closed command-family binding carried by transition records. |
| [duplication.rs](duplication.rs) | Explicit duplication for unprivileged owned command evidence. |
| [permission.rs](permission.rs) | Owned exact unprivileged permission evidence. |
| [use_projection.rs](use_projection.rs) | Complete owned projection of a move-only policy-use command. |

Owning module: [binding.rs](../binding.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
