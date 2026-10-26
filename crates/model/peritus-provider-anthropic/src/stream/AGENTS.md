# crates/model/peritus-provider-anthropic/src/stream — navigation

## Responsibility

Implementation pieces for `stream.rs`: Cancellation-aware pull stream over bounded Anthropic SSE framing.

## Files in this folder

| File | Role |
| --- | --- |
| [content.rs](content.rs) | Ordered Anthropic content-block and delta normalization. |
| [message.rs](message.rs) | Anthropic message-level event normalization. |
| [state.rs](state.rs) | Anthropic SSE event dispatch, deduplication, and normalized envelope ownership. |
| [value.rs](value.rs) | Bounded JSON extraction and normalized metadata helpers. |

Owning module: [stream.rs](../stream.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
