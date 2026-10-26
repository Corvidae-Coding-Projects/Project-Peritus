# crates/model/peritus-provider-compatible/src/request — navigation

## Responsibility

Implementation pieces for `request.rs`: Fail-closed validation and private wire projection for both compatible dialects.

## Files in this folder

| File | Role |
| --- | --- |
| [chat.rs](chat.rs) | Encodes compatible Chat requests, messages, images, and host tool schemas. |
| [hosted.rs](hosted.rs) | Reviewed hosted Chat Completions request projections and bounded reasoning replay. |
| [responses.rs](responses.rs) | Encodes compatible Responses requests and message/tool/image content. |
| [validation.rs](validation.rs) | Checks portable request blocks and canonical input before transport. |
| [value.rs](value.rs) | Small JSON object/string/optional-field construction primitives for request encoders. |

Owning module: [request.rs](../request.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
