# crates/model/peritus-provider-compatible/src/stream/chat — navigation

## Responsibility

Implementation pieces for `chat.rs`: Chat stream decoder and required-tool-choice failure handling.

## Files in this folder

| File | Role |
| --- | --- |
| [fields.rs](fields.rs) | Chat chunk field validation, usage, and aggregate bounds. |
| [hosted.rs](hosted.rs) | Hosted accounting, provider errors, and exact reasoning replay completion. |
| [tools.rs](tools.rs) | Fragmented Chat tool identities, arguments, and completion state. |

Owning module: [chat.rs](../chat.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
