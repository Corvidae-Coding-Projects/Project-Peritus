# crates/app/peritus-daemon/src/artifact — navigation

## Responsibility

Implementation pieces for `artifact.rs`: Authority-owned upload/download transfer services.

## Files in this folder

| File | Role |
| --- | --- |
| [client.rs](client.rs) | Per-connection transfer ownership and bounded download event pumping. |
| [publication.rs](publication.rs) | Deterministic C0 fact recorded before an uploaded object is finalized. |
| [scope.rs](scope.rs) | Durable private attachment claims, created before accepting scoped upload bytes. |
| [service.rs](service.rs) | Serialized transfer registry co-owned with the journal and artifact catalog. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [service/](service/AGENTS.md) | Implementation pieces for `service.rs`: Serialized transfer registry co-owned with the journal and artifact catalog. |

Owning module: [artifact.rs](../artifact.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
