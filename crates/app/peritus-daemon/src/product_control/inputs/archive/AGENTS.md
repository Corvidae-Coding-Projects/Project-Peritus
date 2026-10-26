# crates/app/peritus-daemon/src/product_control/inputs/archive — navigation

## Responsibility

Implementation pieces for `archive.rs`: Private immutable request artifacts installed with the control receipt.

## Files in this folder

| File | Role |
| --- | --- |
| [inspect.rs](inspect.rs) | Content-free projection of a verified manifest; never decodes private model content. |

Owning module: [archive.rs](../archive.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
