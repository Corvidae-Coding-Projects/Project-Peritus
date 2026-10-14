# crates/app/peritus-app-protocol/src/workbench/files — navigation

## Responsibility

Implementation pieces for `files.rs`: Explicit workspace-file preview and selection. DTOs never confer read authority.

## Files in this folder

| File | Role |
| --- | --- |
| [import.rs](import.rs) | Explicit client-imported snapshots. Source labels are never reopened by the daemon. |
| [page.rs](page.rs) | Revision-fenced file selection pages; all historical bytes remain in immutable archives. |
| [preview.rs](preview.rs) | Content-free exact observation and provider-bound confirmation preview. |

Owning module: [files.rs](../files.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
