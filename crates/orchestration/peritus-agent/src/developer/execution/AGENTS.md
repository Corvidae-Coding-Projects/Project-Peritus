# crates/orchestration/peritus-agent/src/developer/execution — navigation

## Responsibility

Provider-turn execution and current invocation/retry policy. Preserve current policy separately from compactable history.

## Files in this folder

| File | Role |
| --- | --- |
| [invocation.rs](invocation.rs) | Current executor lifecycle projected independently of replayed recovery instructions. |
| [provider_turn.rs](provider_turn.rs) | Bounded provider retries and durable public-text delivery. |
| [state.rs](state.rs) | Current input fencing, message construction, and provider-terminal classification. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [provider_turn/](provider_turn/AGENTS.md) | Implementation pieces for `provider_turn.rs`: Bounded provider retries and durable public-text delivery. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
