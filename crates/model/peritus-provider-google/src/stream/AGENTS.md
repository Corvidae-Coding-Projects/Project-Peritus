# crates/model/peritus-provider-google/src/stream — navigation

## Responsibility

Implementation pieces for `stream.rs`: Cancellation-aware pull stream over bounded Google stable-v1 SSE framing.

## Files in this folder

| File | Role |
| --- | --- |
| [generate.rs](generate.rs) | Stable-v1 Generate Content streamed response and candidate normalization. |
| [interaction_fields.rs](interaction_fields.rs) | Bounded field helpers for the Interactions streaming grammar. |
| [interactions.rs](interactions.rs) | Stable-v1 Interactions event grammar and step normalization. |
| [state.rs](state.rs) | Dialect dispatch, exact event deduplication, and normalized envelope ownership. |
| [value.rs](value.rs) | Checked Google stream identities, fragments, usage, cache, and ancillary observations. |

Owning module: [stream.rs](../stream.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
