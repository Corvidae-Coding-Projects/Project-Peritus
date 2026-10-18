# Peritus orchestration guide

## Folder guide

Agent/provider turns, role views, context/memory, scheduling, collaboration, review, gate planning, candidate settlement, and durable domain drivers.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-agent/](peritus-agent/AGENTS.md) | Pure deterministic D0 agent-turn state machine. |
| [peritus-collaboration/](peritus-collaboration/AGENTS.md) | Durable deterministic causal delegation, messaging, joins, and cancellation. |
| [peritus-context/](peritus-context/AGENTS.md) | provenance-aware context planning for Peritus. |
| [peritus-gates/](peritus-gates/AGENTS.md) | Durable deterministic acceptance gate planning, execution, recovery, and evidence. |
| [peritus-harness/](peritus-harness/AGENTS.md) | Durable checked harness definitions and exact workspace materialization. |
| [peritus-modules/](peritus-modules/AGENTS.md) | Bundled and installable agent workflow modules. |
| [peritus-memory/](peritus-memory/AGENTS.md) | scoped derived-memory lifecycle and retrieval for Peritus. |
| [peritus-obligations/](peritus-obligations/AGENTS.md) | public requirement obligations, evidence qualification, and failure ownership. |
| [peritus-orchestrator/](peritus-orchestrator/AGENTS.md) | Durable deterministic writer, gate, reviewer, fixer, and acceptance orchestration. |
| [peritus-review/](peritus-review/AGENTS.md) | Durable deterministic review cycles, finding lifecycle, quorum, and reconciliation. |
| [peritus-role/](peritus-role/AGENTS.md) | role-aware context and capability views for Peritus. |
| [peritus-run-knowledge/](peritus-run-knowledge/AGENTS.md) | digest-bound run knowledge and delta planning for Peritus. |
| [peritus-run-settlement/](peritus-run-settlement/AGENTS.md) | candidate-checkpoint and run-settlement domain for Peritus. |
| [peritus-scheduler/](peritus-scheduler/AGENTS.md) | Durable deterministic bounded resource scheduling and worker ownership. |

