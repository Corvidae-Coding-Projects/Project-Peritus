# crates/app/peritus-product-runner/src/gates/explicit_paths — navigation

## Responsibility

Implementation pieces for `explicit_paths.rs`: Deterministic reconciliation of literal task paths with the candidate workspace.

## Files in this folder

| File | Role |
| --- | --- |
| [alternatives.rs](alternatives.rs) | Recognition of explicit output paths joined by alternative cardinality. |
| [extraction.rs](extraction.rs) | Chronological reduction of literal output and removal instructions. |
| [language.rs](language.rs) | Clause and naming cues for literal path requirements. |
| [removals.rs](removals.rs) | Literal removal directives and their final-state checks; never executes cleanup. |

Owning module: [explicit_paths.rs](../explicit_paths.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
