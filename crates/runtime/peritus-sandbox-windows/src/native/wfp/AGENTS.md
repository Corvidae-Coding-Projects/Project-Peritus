# crates/runtime/peritus-sandbox-windows/src/native/wfp — navigation

## Responsibility

Implementation pieces for `wfp.rs`: Dynamic WFP session that permits only the exact AppContainer-to-proxy route.

## Files in this folder

| File | Role |
| --- | --- |
| [keys.rs](keys.rs) | Deterministic GUID projection for one dynamic WFP session. |

Owning module: [wfp.rs](../wfp.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
