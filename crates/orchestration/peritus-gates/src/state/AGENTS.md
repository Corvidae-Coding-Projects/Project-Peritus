# crates/orchestration/peritus-gates/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Complete authoritative D1 run state.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoint.rs](checkpoint.rs) | Private reconstruction from a fully decoded D1 checkpoint frame. |
| [mutation.rs](mutation.rs) | Reducer-only mutation helpers for otherwise immutable state surfaces. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
