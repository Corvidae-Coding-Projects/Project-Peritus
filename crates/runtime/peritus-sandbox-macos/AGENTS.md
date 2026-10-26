# crates/runtime/peritus-sandbox-macos — navigation

## Responsibility

Deterministic macOS Seatbelt preparation and supervised native sandbox lifecycle.

Package `peritus-sandbox-macos` is a member of the root Cargo workspace; architectural owner `C3` in `runtime`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-network](../peritus-network/AGENTS.md), [peritus-process](../peritus-process/AGENTS.md), [peritus-sandbox](../peritus-sandbox/AGENTS.md), [peritus-secrets](../peritus-secrets/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-sandbox-macos`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | MacOS 15+ native sandbox preparation. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
