# crates/app/peritus-app-client — navigation

## Responsibility

Shared local application-protocol client for Peritus interfaces.

Package `peritus-app-client` is a member of the root Cargo workspace; architectural owner `G1` in `app`.

Local dependency waypoints: [peritus-app-protocol](../peritus-app-protocol/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-app-client`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Shared local transport contracts for Peritus user interfaces. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
