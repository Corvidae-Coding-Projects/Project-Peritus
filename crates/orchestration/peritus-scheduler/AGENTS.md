# crates/orchestration/peritus-scheduler — navigation

## Responsibility

Durable deterministic bounded resource scheduling and worker ownership.

Package `peritus-scheduler` is a member of the root Cargo workspace; architectural owner `D3` in `orchestration`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-scheduler`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic bounded resource scheduling for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
