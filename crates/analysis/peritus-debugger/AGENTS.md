# crates/analysis/peritus-debugger — navigation

## Responsibility

Durable evidence-linked trace diagnosis and harness analysis for Peritus.

Package `peritus-debugger` is a member of the root Cargo workspace; architectural owner `E2` in `analysis`.

Local dependency waypoints: [peritus-eval](../peritus-eval/AGENTS.md), [peritus-agent](../../orchestration/peritus-agent/AGENTS.md), [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-context](../../orchestration/peritus-context/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-harness](../../orchestration/peritus-harness/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-model-protocol](../../model/peritus-model-protocol/AGENTS.md), [peritus-orchestrator](../../orchestration/peritus-orchestrator/AGENTS.md), [peritus-provider-core](../../model/peritus-provider-core/AGENTS.md), [peritus-trace](../../observe/peritus-trace/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-debugger`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable evidence-linked trace diagnosis for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
