# crates/foundation/peritus-protocol/src/schema — navigation

## Responsibility

Reproducible schema, client declaration, and compatibility-corpus generation.

## Files in this folder

| File | Role |
| --- | --- |
| [codegen.rs](codegen.rs) | Filesystem driver for deterministic protocol artifacts. |
| [fixtures.rs](fixtures.rs) | Deterministic version-one compatibility frames. |
| [lifecycle.rs](lifecycle.rs) | Stable B0 lifecycle discriminant metadata for generated clients. |
| [mod.rs](mod.rs) | Reproducible schema, client declaration, and compatibility-corpus generation. |
| [registry.rs](registry.rs) | Stable message-family registry. |
| [render.rs](render.rs) | Deterministic checked-in artifact renderers. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [registry/](registry/AGENTS.md) | Implementation pieces for `registry.rs`: Stable message-family registry. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
