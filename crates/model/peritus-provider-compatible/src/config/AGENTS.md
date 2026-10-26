# crates/model/peritus-provider-compatible/src/config — navigation

## Responsibility

Implementation pieces for `config.rs`: Exact endpoint, header authentication, fixed headers, limits, and retry configuration.

## Files in this folder

| File | Role |
| --- | --- |
| [mappings.rs](mappings.rs) | Explicit retry and response-header mappings; no provider convention is inferred. |

Owning module: [config.rs](../config.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
