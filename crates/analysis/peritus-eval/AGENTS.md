# crates/analysis/peritus-eval — navigation

## Responsibility

Durable reproducible harness evaluation and statistical evidence for Peritus.

Package `peritus-eval` is a member of the root Cargo workspace; architectural owner `E3` in `analysis`.

Local dependency waypoints: [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-harness](../../orchestration/peritus-harness/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-model-protocol](../../model/peritus-model-protocol/AGENTS.md), [peritus-scheduler](../../orchestration/peritus-scheduler/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-eval`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable reproducible harness evaluation for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
