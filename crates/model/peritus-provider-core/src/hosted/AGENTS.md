# crates/model/peritus-provider-core/src/hosted — navigation

## Responsibility

Implementation pieces for `hosted.rs`: Reviewed hosted API identities and operation routes; never a fallback model catalog.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Authenticated service inventory enriched by `OpenCode`'s live, credential-free metadata. |

Owning module: [hosted.rs](../hosted.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
