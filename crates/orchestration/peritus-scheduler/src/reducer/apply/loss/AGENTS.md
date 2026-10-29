# crates/orchestration/peritus-scheduler/src/reducer/apply/loss — navigation

## Responsibility

Implementation pieces for `loss.rs`: Exact worker-loss selection and classification over pre-transition records.

## Files in this folder

| File | Role |
| --- | --- |
| [batch.rs](batch.rs) | Ordered worker-loss release batch and exact lost-worker transition. |
| [release_one.rs](release_one.rs) | One exact reservation release during worker-loss application. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [batch/](batch/AGENTS.md) | Implementation pieces for `batch.rs`: Ordered worker-loss release batch and exact lost-worker transition. |

Owning module: [loss.rs](../loss.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
