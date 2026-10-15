# crates/state/peritus-leases/src/transition/fencing — navigation

## Responsibility

Implementation pieces for `fencing.rs`: Holder-loss, clock-discontinuity, and revocation fencing reducers.

## Files in this folder

| File | Role |
| --- | --- |
| [expiry.rs](expiry.rs) | Deadline-expiry reducer and exact command decision proof. |
| [release.rs](release.rs) | Voluntary release reducer and exact command decision proof. |

Owning module: [fencing.rs](../fencing.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
