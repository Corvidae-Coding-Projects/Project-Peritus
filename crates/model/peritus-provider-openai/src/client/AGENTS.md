# crates/model/peritus-provider-openai/src/client — navigation

## Responsibility

Implementation pieces for `client.rs`: Provider composition, credential timing, HTTP submission, and stream construction.

## Files in this folder

| File | Role |
| --- | --- |
| [cancel.rs](cancel.rs) | Provider-confirmed cancellation for known stored background responses. |
| [catalog.rs](catalog.rs) | Authenticated model discovery on the configured first-party route. |
| [response.rs](response.rs) | Bounded response-body handling and normalized pre-stream failures. |

Owning module: [client.rs](../client.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
