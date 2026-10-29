# crates/orchestration/peritus-agent/src/developer/execution — navigation

## Responsibility

Provider-turn execution and current invocation/retry policy. Preserve current policy separately from compactable history.

## Files in this folder

| File | Role |
| --- | --- |
| [invocation.rs](invocation.rs) | Stable system policy and separate current developer guidance, including executor prerequisites and retries; counters stay out of the cacheable policy prefix. |
| [provider_turn.rs](provider_turn.rs) | Bounded provider retries and durable public-text delivery. |
| [state.rs](state.rs) | Current input fencing, message construction, and provider-terminal classification. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [provider_turn/](provider_turn/AGENTS.md) | Implementation pieces for `provider_turn.rs`: Bounded provider retries and durable public-text delivery. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
