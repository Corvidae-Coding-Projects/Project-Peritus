# crates/app/peritus-app-protocol/src/schema/render — navigation

## Responsibility

Implementation pieces for `render.rs`: Deterministic JSON Schema, TypeScript, and registry renderers.

## Files in this folder

| File | Role |
| --- | --- |
| [json.rs](json.rs) | Draft 2020-12 JSON Schema rendering. |
| [registry_doc.rs](registry_doc.rs) | Human-readable application registry rendering. |
| [typescript.rs](typescript.rs) | TypeScript declaration rendering for the lossless JSON projection. |

Owning module: [render.rs](../render.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
