# crates/app/peritus-product-runner/src/execution/conversation — navigation

## Responsibility

Implementation pieces for `conversation.rs`: Read-only conversational front end to the existing production execution pipeline.

## Files in this folder

| File | Role |
| --- | --- |
| [design.rs](design.rs) | Conversational interview policy and private design-draft tool; cannot mutate the workspace or launch implementation. |
| [tools.rs](tools.rs) | A read-only conversation can hand authorized implementation to the production pipeline. |

Owning module: [conversation.rs](../conversation.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
