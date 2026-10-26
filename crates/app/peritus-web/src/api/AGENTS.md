# crates/app/peritus-web/src/api — navigation

## Responsibility

Implementation pieces for `api.rs`: Authenticated same-origin HTTP routes; all mutation sources share one dispatcher. Entry names: `router`, `protect`, `bootstrap`, `QueryArgs`, `query`.

## Files in this folder

| File | Role |
| --- | --- |
| [file_response.rs](file_response.rs) | Project-confined streaming responses and media-specific browser policy. |

Owning module: [api.rs](../api.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
