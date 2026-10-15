# crates/orchestration/peritus-harness/src/manifest — navigation

## Responsibility

Strict schema-v1 manifest parsing and C1-backed inventory loading.

## Files in this folder

| File | Role |
| --- | --- |
| [document.rs](document.rs) | Strict TOML schema-v1 document and checked domain conversion. |
| [error.rs](error.rs) | Typed strict-manifest and inventory failures. |
| [inventory.rs](inventory.rs) | Recursive deterministic C1 inventory of the harness component root. |
| [loader.rs](loader.rs) | Exact manifest and component loading through `ReadOnlyWorkspace` only. |
| [mod.rs](mod.rs) | Strict schema-v1 manifest parsing and C1-backed inventory loading. |
| [tags.rs](tags.rs) | Strict textual manifest tags mapped to the closed domain catalogs. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
