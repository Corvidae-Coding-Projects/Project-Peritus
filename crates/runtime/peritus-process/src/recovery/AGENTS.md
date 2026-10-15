# crates/runtime/peritus-process/src/recovery — navigation

## Responsibility

Implementation pieces for `recovery.rs`: Durable process registry restart reconciliation.

## Files in this folder

| File | Role |
| --- | --- |
| [claim.rs](claim.rs) | Canonical durable one-use process consumption claims. |
| [manifest.rs](manifest.rs) | Canonical durable process ownership manifest. |
| [reconcile.rs](reconcile.rs) | Exact process-tree probing and deterministic restart classifications. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Canonical durable process ownership manifest. |

Owning module: [recovery.rs](../recovery.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
