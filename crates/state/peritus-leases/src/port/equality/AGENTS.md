# crates/state/peritus-leases/src/port/equality — navigation

## Responsibility

Implementation pieces for `equality.rs`: Semantic equality used at the untrusted durable-CAS boundary.

## Files in this folder

| File | Role |
| --- | --- |
| [aggregate.rs](aggregate.rs) | Full aggregate semantic equality. |
| [binding.rs](binding.rs) | Exact equality for the closed lease command-binding family. |
| [record.rs](record.rs) | Full transition-record and source-command equality. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [binding/](binding/AGENTS.md) | Implementation pieces for `binding.rs`: Exact equality for the closed lease command-binding family. |

Owning module: [equality.rs](../equality.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
