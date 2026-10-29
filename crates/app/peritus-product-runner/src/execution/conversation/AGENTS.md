# crates/app/peritus-product-runner/src/execution/conversation — navigation

## Responsibility

Implementation pieces for `conversation.rs`: Read-only conversational front end to the existing production execution pipeline.

## Files in this folder

| File | Role |
| --- | --- |
| [design.rs](design.rs) | Conversational interview policy and private design-draft tool; cannot mutate the workspace or launch implementation. |
| [handoff.rs](handoff.rs) | Checks direct pipeline-execution promises against actual handoff receipts and supplies an in-session correction without granting authority. |
| [tools.rs](tools.rs) | A read-only conversation can hand authorized implementation to the production pipeline. |

Owning module: [conversation.rs](../conversation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
