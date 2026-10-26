# crates/app/peritus-app-protocol/src/schema — navigation

## Responsibility

Deterministic application-protocol schema and compatibility artifact generation.

## Files in this folder

| File | Role |
| --- | --- |
| [codegen.rs](codegen.rs) | Filesystem driver for deterministic A3 schemas and compatibility fixtures. |
| [fields.rs](fields.rs) | Canonical field and nested-type metadata for the schema-v1 JSON projection. |
| [fixtures.rs](fixtures.rs) | Deterministic classified A3 compatibility frames. |
| [mod.rs](mod.rs) | Deterministic application-protocol schema and compatibility artifact generation. |
| [registry.rs](registry.rs) | Stable application family, payload, and error-code allocation registry. |
| [render.rs](render.rs) | Deterministic JSON Schema, TypeScript, and registry renderers. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [fields/](fields/AGENTS.md) | Implementation pieces for `fields.rs`: Canonical field and nested-type metadata for the schema-v1 JSON projection. |
| [fixtures/](fixtures/AGENTS.md) | Implementation pieces for `fixtures.rs`: Deterministic classified A3 compatibility frames. |
| [render/](render/AGENTS.md) | Implementation pieces for `render.rs`: Deterministic JSON Schema, TypeScript, and registry renderers. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
