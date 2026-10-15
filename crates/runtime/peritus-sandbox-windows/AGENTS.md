# crates/runtime/peritus-sandbox-windows — navigation

## Responsibility

Fail-closed Windows native sandbox preparation, helper, observation, and recovery.

Package `peritus-sandbox-windows` is a member of the root Cargo workspace; architectural owner `C3` in `runtime`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-network](../peritus-network/AGENTS.md), [peritus-process](../peritus-process/AGENTS.md), [peritus-sandbox](../peritus-sandbox/AGENTS.md), [peritus-secrets](../peritus-secrets/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-sandbox-windows`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Windows 11 24H2 and Server 2025 native sandbox preparation. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
