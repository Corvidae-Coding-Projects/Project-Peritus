# crates/app/peritus-app-protocol/src/wire/workbench — navigation

## Responsibility

Implementation pieces for `workbench.rs`: Exact additive typed control codecs; no raw JSON or prompt macro crosses A3.

## Files in this folder

| File | Role |
| --- | --- |
| [intent.rs](intent.rs) | Stable tag routing for typed workbench intents. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [intent/](intent/AGENTS.md) | Implementation pieces for `intent.rs`: Stable tag routing for typed workbench intents. |

Owning module: [workbench.rs](../workbench.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
