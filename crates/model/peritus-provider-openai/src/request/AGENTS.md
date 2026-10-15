# crates/model/peritus-provider-openai/src/request — navigation

## Responsibility

Implementation pieces for `request.rs`: Responses request planning, validation, and HTTP construction.

## Files in this folder

| File | Role |
| --- | --- |
| [input.rs](input.rs) | Ordered message, content, media, tool-result, and replay projection. |
| [options.rs](options.rs) | Tool, structured-output, reasoning, sampling, and cache projection. |
| [value.rs](value.rs) | Explicit JSON value construction without code-generating macros. |

Owning module: [request.rs](../request.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
