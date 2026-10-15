# crates/foundation/peritus-budget/src/transition/accounting — navigation

## Responsibility

Implementation pieces for `accounting.rs`: Shared exact accounting and identity lookup primitives.

## Files in this folder

| File | Role |
| --- | --- |
| [charge_safety.rs](charge_safety.rs) | Executable witnesses that reserved or available capacity makes lineage charging representable. |
| [fault.rs](fault.rs) | Ancestor fault propagation for ambiguous or exceeded reservations. |
| [fault_safety.rs](fault_safety.rs) | Executable reachability witnesses for infallible ancestor fault propagation. |
| [lineage.rs](lineage.rs) | Open-lineage validation and exact reservation-outstanding queries. |
| [lineage_charge.rs](lineage_charge.rs) | Recursive exact consumption propagation through one account lineage. |
| [lookup.rs](lookup.rs) | Exact account/reservation lookup and live-work validation. |
| [receipts.rs](receipts.rs) | Exact construction of unprivileged budget receipts. |
| [releases.rs](releases.rs) | Exact release of outstanding operation reservations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [charge_safety/](charge_safety/AGENTS.md) | Implementation pieces for `charge_safety.rs`: Executable witnesses that reserved or available capacity makes lineage charging representable. |

Owning module: [accounting.rs](../accounting.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
