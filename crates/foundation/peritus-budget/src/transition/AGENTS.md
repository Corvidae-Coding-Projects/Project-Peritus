# crates/foundation/peritus-budget/src/transition — navigation

## Responsibility

Implementation pieces for `transition.rs`: Total value-in/value-out command reducer.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Shared exact accounting and identity lookup primitives. |
| [allocation.rs](allocation.rs) | Child allocation and atomic operation begin transitions. |
| [lifecycle.rs](lifecycle.rs) | Account sealing, closure, and begin-shape validation. |
| [reconciliation.rs](reconciliation.rs) | Reservation activation, observation, and terminal reconciliation. |
| [reducer.rs](reducer.rs) | Closed command dispatch into command-local exact reducers. |
| [validation.rs](validation.rs) | Executable tree, tombstone, conservation, and refinement validation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [accounting/](accounting/AGENTS.md) | Implementation pieces for `accounting.rs`: Shared exact accounting and identity lookup primitives. |
| [allocation/](allocation/AGENTS.md) | Implementation pieces for `allocation.rs`: Child allocation and atomic operation begin transitions. |
| [lifecycle/](lifecycle/AGENTS.md) | Implementation pieces for `lifecycle.rs`: Account sealing, closure, and begin-shape validation. |
| [reconciliation/](reconciliation/AGENTS.md) | Implementation pieces for `reconciliation.rs`: Reservation activation, observation, and terminal reconciliation. |
| [validation/](validation/AGENTS.md) | Implementation pieces for `validation.rs`: Executable tree, tombstone, conservation, and refinement validation. |

Owning module: [transition.rs](../transition.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
