# crates/foundation/peritus-budget/src/reachability/functionality/observation — navigation

## Responsibility

Implementation pieces for `observation.rs`: Functionality of high-water observation candidates.

## Files in this folder

| File | Role |
| --- | --- |
| [applied.rs](applied.rs) | Exact functionality of accepted in-ceiling high-water observations. |
| [overrun.rs](overrun.rs) | Exact functionality of accepted over-ceiling observation candidates. |
| [overrun_witness.rs](overrun_witness.rs) | Witness projections for exact active-overrun accounting. |

Owning module: [observation.rs](../observation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
