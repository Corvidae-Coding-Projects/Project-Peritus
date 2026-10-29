# crates/tools/peritus-tools-computer — navigation

## Responsibility

Persistent browser and desktop automation host adapters for Peritus.

Package `peritus-tools-computer` is a member of the root Cargo workspace.

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-computer`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Browser/desktop control host adapters, persistent session RPC, installation/setup, screenshots, observations, and controller driver sources. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
