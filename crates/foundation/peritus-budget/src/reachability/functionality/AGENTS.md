# crates/foundation/peritus-budget/src/reachability/functionality — navigation

## Responsibility

Implementation pieces for `functionality.rs`: Functionality of the exact command-candidate relation.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Functionality of recursive lineage and single-account accounting effects. |
| [allocation.rs](allocation.rs) | Functionality of child-allocation candidates. |
| [dispatch.rs](dispatch.rs) | Closed per-command dispatch for candidate functionality. |
| [fault.rs](fault.rs) | Functionality of ancestor fault propagation. |
| [finalization.rs](finalization.rs) | Functionality of reservation terminalization candidates. |
| [lifecycle.rs](lifecycle.rs) | Functionality of account lifecycle candidates. |
| [observation.rs](observation.rs) | Functionality of high-water observation candidates. |
| [operations.rs](operations.rs) | Functionality of begin-operation candidates. |
| [release.rs](release.rs) | Functionality of reservation-release accounting effects. |
| [reservation_updates.rs](reservation_updates.rs) | Functionality of single-record reservation effects. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [finalization/](finalization/AGENTS.md) | Implementation pieces for `finalization.rs`: Functionality of reservation terminalization candidates. |
| [observation/](observation/AGENTS.md) | Implementation pieces for `observation.rs`: Functionality of high-water observation candidates. |

Owning module: [functionality.rs](../functionality.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
