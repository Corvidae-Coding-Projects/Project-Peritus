# crates/orchestration/peritus-review/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Complete authoritative D2 review-run state.

## Files in this folder

| File | Role |
| --- | --- |
| [mutation.rs](mutation.rs) | Reducer-only mutation helpers for otherwise immutable D2 state. |
| [terminal.rs](terminal.rs) | Truthful immutable terminal summary for a D2 review aggregate. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
