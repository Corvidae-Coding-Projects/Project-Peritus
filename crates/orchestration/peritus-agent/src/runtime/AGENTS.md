# crates/orchestration/peritus-agent/src/runtime — navigation

## Responsibility

Ordinary effect composition around the verified agent reducer.

## Files in this folder

| File | Role |
| --- | --- |
| [budget.rs](budget.rs) | B1 reservation planning and cumulative model/tool usage settlement. |
| [context.rs](context.rs) | C6 role, memory, context-selection, and provider-neutral rendering composition. |
| [driver.rs](driver.rs) | Cooperative durable composition of the pure reducer and ordinary effect adapters. |
| [durability.rs](durability.rs) | Atomic B3/C0 persistence and restart loading for one D0 aggregate. |
| [mod.rs](mod.rs) | Ordinary effect composition around the verified agent reducer. |
| [model.rs](model.rs) | Pull-based C5 model execution with an explicit durable-acknowledgement boundary. |
| [tools.rs](tools.rs) | C4-only preparation, authority handoff, bounded execution, control, and result ordering. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [budget/](budget/AGENTS.md) | Implementation pieces for `budget.rs`: B1 reservation planning and cumulative model/tool usage settlement. |
| [driver/](driver/AGENTS.md) | Implementation pieces for `driver.rs`: Cooperative durable composition of the pure reducer and ordinary effect adapters. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
