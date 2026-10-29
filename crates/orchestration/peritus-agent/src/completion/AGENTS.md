# crates/orchestration/peritus-agent/src/completion — navigation

## Responsibility

Implementation pieces for `completion.rs`: Evidence-bound completion proposals with no effect authority.

## Files in this folder

| File | Role |
| --- | --- |
| [clone_impl.rs](clone_impl.rs) | Semantic clone for retained completion data. |

Owning module: [completion.rs](../completion.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
