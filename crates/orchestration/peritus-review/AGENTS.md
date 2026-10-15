# crates/orchestration/peritus-review — navigation

## Responsibility

Durable deterministic review cycles, finding lifecycle, quorum, and reconciliation.

Package `peritus-review` is a member of the root Cargo workspace; architectural owner `D2` in `orchestration`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-context](../peritus-context/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-quality-policy](../../foundation/peritus-quality-policy/AGENTS.md), [peritus-role](../peritus-role/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-review`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic review cycles and finding lifecycle for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
