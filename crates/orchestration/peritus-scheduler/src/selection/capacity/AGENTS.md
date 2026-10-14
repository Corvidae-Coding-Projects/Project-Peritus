# crates/orchestration/peritus-scheduler/src/selection/capacity — navigation

## Responsibility

Implementation pieces for `capacity.rs`: Exact checked reservation sums and prospective capacity predicates.

## Files in this folder

| File | Role |
| --- | --- |
| [aggregate.rs](aggregate.rs) | Canonical aggregate resource sums with exact fallback semantics. |
| [fallback.rs](fallback.rs) | Exact per-dimension fallback when canonical aggregate storage cannot represent a sum. |
| [refinement.rs](refinement.rs) | Refinement from finite request-entry checks to all resource kinds. |

Owning module: [capacity.rs](../capacity.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
