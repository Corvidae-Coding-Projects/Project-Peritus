# crates/state/peritus-leases/src/port — navigation

## Responsibility

Implementation pieces for `port.rs`: Durable compare-and-swap request and untrusted observation contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [contracts.rs](contracts.rs) | Exact request construction, untrusted echo validation, and resolution contracts. |
| [equality.rs](equality.rs) | Semantic equality used at the untrusted durable-CAS boundary. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [equality/](equality/AGENTS.md) | Implementation pieces for `equality.rs`: Semantic equality used at the untrusted durable-CAS boundary. |

Owning module: [port.rs](../port.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
