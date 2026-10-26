# crates/app/peritus-product-runner/src/attachment — navigation

## Responsibility

Implementation pieces for `attachment.rs`: Validation of explicitly selected immutable image bytes, without filesystem authority.

## Files in this folder

| File | Role |
| --- | --- |
| [decode.rs](decode.rs) | Incremental pixel validation; metadata alone is not acceptance evidence. |
| [text.rs](text.rs) | Exact bounded UTF-8 attachment bytes; validation never rewrites or truncates the source. |

Owning module: [attachment.rs](../attachment.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
