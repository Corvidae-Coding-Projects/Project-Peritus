# crates/orchestration/peritus-gates — navigation

## Responsibility

Durable deterministic acceptance gate planning, execution, recovery, and evidence.

Package `peritus-gates` is a member of the root Cargo workspace; architectural owner `D1` in `orchestration`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-obligations](../peritus-obligations/AGENTS.md), [peritus-quality-policy](../../foundation/peritus-quality-policy/AGENTS.md), [peritus-run-settlement](../peritus-run-settlement/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-tool-protocol](../../tools/peritus-tool-protocol/AGENTS.md), [peritus-tools-quality](../../tools/peritus-tools-quality/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-gates`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic acceptance gate orchestration for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
