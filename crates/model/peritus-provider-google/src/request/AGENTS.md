# crates/model/peritus-provider-google/src/request — navigation

## Responsibility

Implementation pieces for `request.rs`: Checked provider-neutral request to private stable-v1 Google JSON projection.

## Files in this folder

| File | Role |
| --- | --- |
| [content.rs](content.rs) | Shared content, tool-schema, and opaque-thinking projection. |
| [generate.rs](generate.rs) | Stable-v1 Generate Content contents, functions, cache, and generation projection. |
| [interactions.rs](interactions.rs) | Stable-v1 Interactions input, steps, tools, state, and generation projection. |
| [value.rs](value.rs) | Explicit JSON construction for the private Google wire boundary. |

Owning module: [request.rs](../request.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
