# crates/app/peritus-daemon/src/product_run/workbench/files — navigation

## Responsibility

Implementation pieces for `files.rs`: Authorized workspace-file preview, confirmation and retained selection inspection.

## Files in this folder

| File | Role |
| --- | --- |
| [import.rs](import.rs) | Client-chosen external text becomes scoped immutable data, never daemon path authority. |
| [mapping.rs](mapping.rs) | Pure A3/source binding; no client can manufacture a host read proof. |
| [page.rs](page.rs) | Authenticated retained-file inspection, without opening source paths. |
| [refresh.rs](refresh.rs) | Coherent request-construction snapshots; preparation retries reuse exact successful sources. |

Owning module: [files.rs](../files.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
