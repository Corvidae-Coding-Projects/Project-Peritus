# crates/tools/peritus-tools-fs/src/render — navigation

## Responsibility

Implementation pieces for `render.rs`: Bounded model, human, and structured filesystem renderings.

## Files in this folder

| File | Role |
| --- | --- |
| [fragments.rs](fragments.rs) | Bounded discovery/search result fragments, omission markers, and fragment cursors. |
| [json.rs](json.rs) | Capacity-aware JSON constructors for rendered filesystem pages. |
| [omission.rs](omission.rs) | Bounded serialization and continuation arithmetic for scope omissions. |
| [pages.rs](pages.rs) | Filesystem output page rendering and continuation metadata. |

Owning module: [render.rs](../render.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
