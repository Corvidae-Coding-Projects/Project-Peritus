# crates/model/peritus-model-protocol/src/reducer — navigation

## Responsibility

Implementation pieces for `reducer.rs`: Ordered bounded event reduction with fail-closed terminal classification.

## Files in this folder

| File | Role |
| --- | --- |
| [events.rs](events.rs) | Started-response event application and item assembly. |
| [stream.rs](stream.rs) | Stream sequencing, deduplication, and public reducer observations. |
| [terminal.rs](terminal.rs) | Completed-item validation and fail-closed terminal classification. |

Owning module: [reducer.rs](../reducer.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
