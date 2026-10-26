# crates/state/peritus-journal/src/domain/approval_use — navigation

## Responsibility

Implementation pieces for `approval_use.rs`: Atomic durable approve-once consumption persistence.

## Files in this folder

| File | Role |
| --- | --- |
| [resolution.rs](resolution.rs) | Exact precondition-independent approval-use command resolution. |

Owning module: [approval_use.rs](../approval_use.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
