# crates/tools/peritus-tools-shell — navigation

## Responsibility

Authorized structured argv and explicit script tools for Peritus.

Package `peritus-tools-shell` is a member of the root Cargo workspace; architectural owner `C4` in `tools`.

Local dependency waypoints: [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-process](../../runtime/peritus-process/AGENTS.md), [peritus-sandbox](../../runtime/peritus-sandbox/AGENTS.md), [peritus-tool-protocol](../peritus-tool-protocol/AGENTS.md), [peritus-tool-router](../peritus-tool-router/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-shell`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Authorized structured argv and explicit script tool adapters. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
