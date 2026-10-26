# crates/app/peritus-tui/src/render/chat/images — navigation

## Responsibility

Implementation pieces for `images.rs`: Readable exact-byte consent preview; pixels are not required for a terminal confirmation.

## Files in this folder

| File | Role |
| --- | --- |
| [page.rs](page.rs) | Exact retained reference details; selection and next-turn eligibility are distinct facts. |

Owning module: [images.rs](../images.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
