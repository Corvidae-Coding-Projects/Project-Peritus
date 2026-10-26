# crates/app/peritus-daemon/src/product_run/workbench/checkpoints — navigation

## Responsibility

Implementation pieces for `checkpoints.rs`: Bounded workspace checkpoints and exact-preimage rewind transactions.

## Files in this folder

| File | Role |
| --- | --- |
| [capture.rs](capture.rs) | Bounded selected-path capture and no-follow folder observations. |
| [logical.rs](logical.rs) | Reserved logical rewind branches, published atomically only after successful settlement. |
| [lookup.rs](lookup.rs) | Exact restore receipt lookup and retry resolution. |
| [pages.rs](pages.rs) | Frame-sized, stateless checkpoint coverage pages. |
| [projection.rs](projection.rs) | Exact checkpoint and restore projections and stable identity derivation. |
| [recovery.rs](recovery.rs) | Prepared rewind reconciliation against exact C1 and retained checkpoint evidence. |
| [rewind.rs](rewind.rs) | Reviewed rewind preview, exact C1 application, and truthful durable settlement. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [capture/](capture/AGENTS.md) | Implementation pieces for `capture.rs`: Bounded selected-path capture and no-follow folder observations. |
| [lookup/](lookup/AGENTS.md) | Implementation pieces for `lookup.rs`: Exact restore receipt lookup and retry resolution. |
| [rewind/](rewind/AGENTS.md) | Implementation pieces for `rewind.rs`: Reviewed rewind preview, exact C1 application, and truthful durable settlement. |

Owning module: [checkpoints.rs](../checkpoints.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
