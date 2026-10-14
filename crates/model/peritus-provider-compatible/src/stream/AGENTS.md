# crates/model/peritus-provider-compatible/src/stream — navigation

## Responsibility

Implementation pieces for `stream.rs`: Bounded SSE ownership and normalized compatible event emission.

## Files in this folder

| File | Role |
| --- | --- |
| [ancillary.rs](ancillary.rs) | Normalizes ancillary provider events and safe response metadata. |
| [chat.rs](chat.rs) | Chat stream decoder and required-tool-choice failure handling. |
| [responses.rs](responses.rs) | Responses stream event/object decoding. |
| [responses_state.rs](responses_state.rs) | Per-item/part stream state and sequence classification for Responses decoding. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [chat/](chat/AGENTS.md) | Implementation pieces for `chat.rs`: Chat stream decoder and required-tool-choice failure handling. |
| [responses/](responses/AGENTS.md) | Implementation pieces for `responses.rs`: Responses stream event/object decoding. |

Owning module: [stream.rs](../stream.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
