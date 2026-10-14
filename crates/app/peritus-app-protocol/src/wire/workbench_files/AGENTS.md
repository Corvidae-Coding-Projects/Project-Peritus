# crates/app/peritus-app-protocol/src/wire/workbench_files — navigation

## Responsibility

Implementation pieces for `workbench_files.rs`: Canonical typed file preview and selection codecs. Paths are inert descriptors.

## Files in this folder

| File | Role |
| --- | --- |
| [import.rs](import.rs) | Canonical explicit-import transfer, preview and exact consent binding. |
| [page.rs](page.rs) | Bounded canonical file pages; row count is checked before allocation. |

Owning module: [workbench_files.rs](../workbench_files.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
