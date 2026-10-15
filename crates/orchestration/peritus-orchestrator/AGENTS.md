# crates/orchestration/peritus-orchestrator — navigation

## Responsibility

Durable deterministic writer, gate, reviewer, fixer, and acceptance orchestration.

Package `peritus-orchestrator` is a member of the root Cargo workspace; architectural owner `E0` in `orchestration`.

Local dependency waypoints: [peritus-agent](../peritus-agent/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-collaboration](../peritus-collaboration/AGENTS.md), [peritus-gates](../peritus-gates/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-kernel](../../foundation/peritus-kernel/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-quality-policy](../../foundation/peritus-quality-policy/AGENTS.md), [peritus-review](../peritus-review/AGENTS.md), [peritus-role](../peritus-role/AGENTS.md), [peritus-scheduler](../peritus-scheduler/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-orchestrator`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic E0 delivery orchestration for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
