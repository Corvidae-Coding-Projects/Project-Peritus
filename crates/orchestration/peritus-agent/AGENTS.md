# crates/orchestration/peritus-agent — navigation

## Responsibility

Pure deterministic D0 agent-turn state machine.

Package `peritus-agent` is a member of the root Cargo workspace; architectural owner `D0` in `orchestration`.

Local dependency waypoints: [peritus-budget](../../foundation/peritus-budget/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-context](../peritus-context/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-memory](../peritus-memory/AGENTS.md), [peritus-model-protocol](../../model/peritus-model-protocol/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-provider-core](../../model/peritus-provider-core/AGENTS.md), [peritus-role](../peritus-role/AGENTS.md), [peritus-tool-protocol](../../tools/peritus-tool-protocol/AGENTS.md), [peritus-tool-router](../../tools/peritus-tool-router/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-agent`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Two connected surfaces: DeveloperLoop drives product provider/tool turns; durable AgentDriver/state-machine modules own journal-backed domain-agent execution. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
