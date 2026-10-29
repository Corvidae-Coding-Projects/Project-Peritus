# crates/app/peritus-product-runner/src/control/images — navigation

## Responsibility

Implementation pieces for `images.rs`: Explicit immutable image references and future-request selection, not filesystem grants.

## Files in this folder

| File | Role |
| --- | --- |
| [metadata.rs](metadata.rs) | Claimed image metadata is bounded but never substitutes for decoded original bytes. |

Owning module: [images.rs](../images.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
