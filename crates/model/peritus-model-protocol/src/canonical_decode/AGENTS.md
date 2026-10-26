# crates/model/peritus-model-protocol/src/canonical_decode — navigation

## Responsibility

Implementation pieces for `canonical_decode.rs`: Bounded fail-closed decoding of version-one canonical model requests.

## Files in this folder

| File | Role |
| --- | --- |
| [content.rs](content.rs) | Checked message, content-block, and media decoding. |
| [options.rs](options.rs) | Checked request-option and policy decoding. |
| [primitive.rs](primitive.rs) | Primitive canonical reads and checked nested domain values. |

Owning module: [canonical_decode.rs](../canonical_decode.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
