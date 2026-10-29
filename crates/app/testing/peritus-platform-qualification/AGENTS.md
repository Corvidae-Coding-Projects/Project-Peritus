# crates/app/testing/peritus-platform-qualification — navigation

## Responsibility

H2 packaged-host qualification contracts and fresh-subject runner for Peritus.

Package `peritus-platform-qualification` is a member of the root Cargo workspace; architectural owner `H2` in `testing`.

Local dependency waypoints: [peritus-approval](../../../state/peritus-approval/AGENTS.md), [peritus-local-socket](../../../runtime/peritus-local-socket/AGENTS.md), [peritus-types](../../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-platform-qualification`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | H2 packaged-host qualification contracts for Linux, macOS, and Windows. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
