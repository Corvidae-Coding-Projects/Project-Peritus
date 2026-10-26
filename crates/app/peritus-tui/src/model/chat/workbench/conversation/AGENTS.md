# crates/app/peritus-tui/src/model/chat/workbench/conversation — navigation

## Responsibility

Implementation pieces for `conversation.rs`: Composer admission through the same immutable queue and exact receipts as /queue.

## Files in this folder

| File | Role |
| --- | --- |
| [message.rs](message.rs) | Chunked immutable user-message admission shares the scoped text artifact contract. |

Owning module: [conversation.rs](../conversation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
