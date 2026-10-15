# crates/app/peritus-daemon/src/instance — navigation

## Responsibility

Implementation pieces for `instance.rs`: Exclusive process instance ownership.

## Files in this folder

| File | Role |
| --- | --- |
| [lock.rs](lock.rs) | Exclusive daemon lock and exact record publication. |
| [record.rs](record.rs) | Canonical durable live-instance record. |

Owning module: [instance.rs](../instance.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
