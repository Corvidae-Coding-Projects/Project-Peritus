# crates/app/peritus-daemon/src/outbox/claims — navigation

## Responsibility

Implementation pieces for `claims.rs`: Typed, effect-free decoding of claimed C0 outbox rows.

## Files in this folder

| File | Role |
| --- | --- |
| [domains.rs](domains.rs) | Native E1, E2, E3, and F0 claim bindings. |
| [orchestrator.rs](orchestrator.rs) | Exact E0 publish-command claim binding for all seven directive destinations. |

Owning module: [claims.rs](../claims.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
