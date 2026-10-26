# crates/app/peritus-tui/src/model/chat/workbench/receipts — navigation

## Responsibility

Implementation pieces for `receipts.rs`: Exact receipt correlation and explicit retry after an uncertain transport outcome.

## Files in this folder

| File | Role |
| --- | --- |
| [correlation.rs](correlation.rs) | Identical exact-operation checks for control responses and read-only receipt lookup. |
| [error.rs](error.rs) | Presentation-state recovery for rejected or uncertain workbench requests. |
| [recovery.rs](recovery.rs) | Resume admission can precede launch persistence; observation must retain exact retry. |
| [rejection.rs](rejection.rs) | Rejected admission must not lock the user out of the controls needed for recovery. |

Owning module: [receipts.rs](../receipts.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
