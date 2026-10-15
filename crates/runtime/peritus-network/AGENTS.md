# crates/runtime/peritus-network — navigation

## Responsibility

outbound connection planning and bounded managed proxy.

Package `peritus-network` is a member of the root Cargo workspace; architectural owner `C3` in `runtime`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-sandbox](../peritus-sandbox/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-network`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Connection planning and bounded managed outbound proxy. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
