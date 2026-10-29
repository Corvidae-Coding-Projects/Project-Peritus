# crates/app/peritus-daemon/src/product_run/workbench/images — navigation

## Responsibility

Implementation pieces for `images.rs`: Authenticated image preview and confirmed atomic import; no ambient files or provider sends.

## Files in this folder

| File | Role |
| --- | --- |
| [mapping.rs](mapping.rs) | Explicit A3/domain image mapping. Checked metadata never substitutes for a decoded proof. |
| [page.rs](page.rs) | Read-only retained image metadata from a verified, authenticated conversation snapshot. |

Owning module: [images.rs](../images.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
