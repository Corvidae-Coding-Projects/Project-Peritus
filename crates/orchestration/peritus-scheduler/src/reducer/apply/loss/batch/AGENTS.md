# crates/orchestration/peritus-scheduler/src/reducer/apply/loss/batch — navigation

## Responsibility

Implementation pieces for `batch.rs`: Ordered worker-loss release batch and exact lost-worker transition.

## Files in this folder

| File | Role |
| --- | --- |
| [release_plan.rs](release_plan.rs) | Finite execution of the exact precomputed worker-loss release plan. |
| [worker_target.rs](worker_target.rs) | Exact worker lookup and final lifecycle-target facts for loss application. |

Owning module: [batch.rs](../batch.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
