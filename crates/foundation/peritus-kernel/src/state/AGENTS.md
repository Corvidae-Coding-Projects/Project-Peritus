# crates/foundation/peritus-kernel/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Closed lifecycle phases and checked state records.

## Files in this folder

| File | Role |
| --- | --- |
| [acceptance.rs](acceptance.rs) | Acceptance lifecycle state. |
| [action.rs](action.rs) | Action lifecycle state. |
| [attempt.rs](attempt.rs) | Attempt lifecycle state. |
| [review.rs](review.rs) | Review-cycle lifecycle state. |
| [run.rs](run.rs) | Run lifecycle state. |
| [session.rs](session.rs) | Session lifecycle state. |
| [turn.rs](turn.rs) | Turn lifecycle state. |
| [waiver.rs](waiver.rs) | Finding-waiver lifecycle state. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
