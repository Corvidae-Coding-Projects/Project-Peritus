# crates/runtime/peritus-workspace/src/inspection — navigation

## Responsibility

Implementation pieces for `inspection.rs`: Structured no-follow inspection of an immutable workspace snapshot.

## Files in this folder

| File | Role |
| --- | --- |
| [directory.rs](directory.rs) | Bounded, deterministic paging of direct workspace directory entries. |

Owning module: [inspection.rs](../inspection.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
