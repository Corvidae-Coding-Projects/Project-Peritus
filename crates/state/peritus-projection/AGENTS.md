# crates/state/peritus-projection — navigation

## Responsibility

Pure deterministic journal projections and durable shadow rebuilds for Peritus.

Package `peritus-projection` is a member of the root Cargo workspace; architectural owner `C0` in `state`.

Local dependency waypoints: [peritus-approval](../peritus-approval/AGENTS.md), [peritus-budget](../../foundation/peritus-budget/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-journal](../peritus-journal/AGENTS.md), [peritus-kernel](../../foundation/peritus-kernel/AGENTS.md), [peritus-leases](../peritus-leases/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-projection`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Pure deterministic journal projections and durable shadow rebuilds for Peritus. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
