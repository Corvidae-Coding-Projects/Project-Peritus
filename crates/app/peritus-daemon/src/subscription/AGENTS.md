# crates/app/peritus-daemon/src/subscription — navigation

## Responsibility

Implementation pieces for `subscription.rs`: Canonical topic compilation and bounded live event delivery.

## Files in this folder

| File | Role |
| --- | --- |
| [pump.rs](pump.rs) | Connection-owned bounded subscription registry and journal tail pump. |
| [registry.rs](registry.rs) | Closed application topic registry. |

Owning module: [subscription.rs](../subscription.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
