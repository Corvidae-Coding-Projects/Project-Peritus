# crates/state/peritus-leases/src/model/concrete/rejections/fencing — navigation

## Responsibility

Implementation pieces for `fencing.rs`: Shared exact last-stage failure model for active-generation fencing.

## Files in this folder

| File | Role |
| --- | --- |
| [discontinuity.rs](discontinuity.rs) | Ordered first-error and total-decision model for authority-clock discontinuity fencing. |
| [expiry.rs](expiry.rs) | Ordered first-error and total-decision model for deadline expiry fencing. |
| [holder_loss.rs](holder_loss.rs) | Ordered first-error and total-decision model for holder-loss fencing. |
| [release.rs](release.rs) | Ordered first-error and total-decision model for voluntary release. |
| [revoke.rs](revoke.rs) | Ordered first-error and total-decision model for authorized revocation fencing. |

Owning module: [fencing.rs](../fencing.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
