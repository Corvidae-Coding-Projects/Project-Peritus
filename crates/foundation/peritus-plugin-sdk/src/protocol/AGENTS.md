# crates/foundation/peritus-plugin-sdk/src/protocol — navigation

## Responsibility

Implementation pieces for `protocol.rs`: Typed host/plugin request, result, lifecycle, and failure protocol.

## Files in this folder

| File | Role |
| --- | --- |
| [request_wire.rs](request_wire.rs) | Manual request-side encoding for the version-one plugin protocol. |
| [response_wire.rs](response_wire.rs) | Manual response-side encoding for the version-one plugin protocol. |
| [wire.rs](wire.rs) | Shared manual Serde helpers for the version-one plugin protocol. |

Owning module: [protocol.rs](../protocol.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
