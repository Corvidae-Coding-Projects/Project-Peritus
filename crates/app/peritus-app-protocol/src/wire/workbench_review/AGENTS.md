# crates/app/peritus-app-protocol/src/wire/workbench_review — navigation

## Responsibility

Implementation pieces for `workbench_review.rs`: Canonical structured-review codecs with bounded collection allocation.

## Files in this folder

| File | Role |
| --- | --- |
| [content.rs](content.rs) | Bounded diff-page content, comment, and evidence codecs. |

Owning module: [workbench_review.rs](../workbench_review.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
