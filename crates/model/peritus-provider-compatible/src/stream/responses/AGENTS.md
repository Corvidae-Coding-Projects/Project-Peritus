# crates/model/peritus-provider-compatible/src/stream/responses — navigation

## Responsibility

Implementation pieces for `responses.rs`: Responses stream event/object decoding.

## Files in this folder

| File | Role |
| --- | --- |
| [terminal.rs](terminal.rs) | Responses lifecycle terminals and final usage normalization. |
| [tools.rs](tools.rs) | Raw tool-fragment ownership, exact terminal binding, and closed-argument normalization. |

Owning module: [responses.rs](../responses.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
