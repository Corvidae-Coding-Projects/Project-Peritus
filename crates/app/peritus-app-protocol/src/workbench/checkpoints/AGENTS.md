# crates/app/peritus-app-protocol/src/workbench/checkpoints — navigation

## Responsibility

Implementation pieces for `checkpoints.rs`: Public bounded checkpoint coverage, rewind previews, and durable restore receipts.

## Files in this folder

| File | Role |
| --- | --- |
| [paging.rs](paging.rs) | Explicit checkpoint coverage cursors and bounded pages. |
| [restore.rs](restore.rs) | Durable restore outcomes and their compact paged-protocol form. |
| [rewind.rs](rewind.rs) | Exact revision-fenced rewind selection, preview, and per-path decisions. |
| [rewind_page.rs](rewind_page.rs) | One bounded page of an exact rewind preview. |

Owning module: [checkpoints.rs](../checkpoints.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
