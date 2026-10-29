# crates/model/peritus-provider-compatible/src/client — navigation

## Responsibility

Implementation pieces for `client.rs`: Provider composition, credential timing, conservative retries, and stream ownership.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Model discovery beside the configured inference route, preserving custom authentication. |
| [metadata.rs](metadata.rs) | Normalizes HTTP success/failure/retry/request-ID metadata. |
| [response.rs](response.rs) | Reads bounded response bodies and selects event-stream/failure output paths. |

Owning module: [client.rs](../client.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
