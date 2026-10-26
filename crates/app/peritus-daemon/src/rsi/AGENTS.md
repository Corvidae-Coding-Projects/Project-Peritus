# crates/app/peritus-daemon/src/rsi — navigation

## Responsibility

Implementation pieces for `rsi.rs`: Native candidate evaluation and evidence-backed production evolution.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Serialized E3/F0 claim ownership. External processes run outside this owner. |
| [completion.rs](completion.rs) | Recovery of committed native work that has no remaining effect directive. |
| [evaluation.rs](evaluation.rs) | Real E3 campaign creation, full-record recovery, analysis, and claimed publication. |
| [evolution.rs](evolution.rs) | Production F0 orchestration over the existing durable owners. |
| [execution.rs](execution.rs) | Credential-free local RSI executions through the committed C2 and native C3 gateway. |
| [local.rs](local.rs) | Actual restricted local candidate/evaluator port for an already committed E3 attempt. |
| [production.rs](production.rs) | Fresh production lookup. Captured bindings remain immutable across later activations. |
| [recovery.rs](recovery.rs) | Startup recovery from real C0 attempt admission and exact C2 child ownership. |
| [runtime.rs](runtime.rs) | Owned native delivery: real processes outside C0, exact settlement through the C0 owner. |
| [scheduling.rs](scheduling.rs) | Native E3 scheduling through replayed, durably committed D3 coordination ownership. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [evolution/](evolution/AGENTS.md) | Implementation pieces for `evolution.rs`: Production F0 orchestration over the existing durable owners. |
| [execution/](execution/AGENTS.md) | Implementation pieces for `execution.rs`: Credential-free local RSI executions through the committed C2 and native C3 gateway. |
| [local/](local/AGENTS.md) | Implementation pieces for `local.rs`: Actual restricted local candidate/evaluator port for an already committed E3 attempt. |

Owning module: [rsi.rs](../rsi.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
