# crates/app/peritus-daemon/src/component/tools — navigation

## Responsibility

Implementation pieces for `tools.rs`: Closed production C4 tool inventory and router composition.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Explicit compiled descriptor inventory; no filesystem or plugin discovery occurs here. |
| [error.rs](error.rs) | Stable startup and runtime composition failures for configured tools. |
| [registry.rs](registry.rs) | Deterministic configured registry and sole stateful C4 router owner. |
| [route.rs](route.rs) | Closed mapping from immutable tool descriptors to production adapter constructors. |
| [selection.rs](selection.rs) | Exact allowlist selection and B1 operation construction. |

Owning module: [tools.rs](../tools.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
