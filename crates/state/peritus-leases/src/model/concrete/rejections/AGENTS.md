# crates/state/peritus-leases/src/model/concrete/rejections — navigation

## Responsibility

Implementation pieces for `rejections.rs`: Exact total decision relations for every executable lease reducer.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Ordered first-error and total-decision model for policy/lease authority intersection. |
| [fencing.rs](fencing.rs) | Shared exact last-stage failure model for active-generation fencing. |
| [lifecycle.rs](lifecycle.rs) | Ordered first-error and total-decision models for mint, acquire, and renew. |
| [reconciliation.rs](reconciliation.rs) | Ordered first-error and total-decision model for fenced-generation reconciliation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [fencing/](fencing/AGENTS.md) | Implementation pieces for `fencing.rs`: Shared exact last-stage failure model for active-generation fencing. |

Owning module: [rejections.rs](../rejections.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
