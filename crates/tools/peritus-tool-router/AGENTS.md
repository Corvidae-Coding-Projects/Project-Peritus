# crates/tools/peritus-tool-router — navigation

## Responsibility

Exact-authority tool registry, router, dispatch, control, and replay boundary.

Package `peritus-tool-router` is a member of the root Cargo workspace; architectural owner `C4` in `tools`.

Local dependency waypoints: [peritus-budget](../../foundation/peritus-budget/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-kernel](../../foundation/peritus-kernel/AGENTS.md), [peritus-leases](../../state/peritus-leases/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-tool-protocol](../peritus-tool-protocol/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tool-router`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Exact-authority C4 tool router and sole invocation-permit owner. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
