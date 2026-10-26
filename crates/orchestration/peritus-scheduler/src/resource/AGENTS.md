# crates/orchestration/peritus-scheduler/src/resource — navigation

## Responsibility

Implementation pieces for `resource.rs`: Checked canonical resource dimensions and arithmetic.

## Files in this folder

| File | Role |
| --- | --- |
| [addition.rs](addition.rs) | Canonical resource addition used by public arithmetic and capacity aggregation. |
| [capacity.rs](capacity.rs) | Exact executable resource quantity and capacity correspondence. |
| [subtraction.rs](subtraction.rs) | Canonical resource subtraction without output revalidation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [addition/](addition/AGENTS.md) | Implementation pieces for `addition.rs`: Canonical resource addition used by public arithmetic and capacity aggregation. |

Owning module: [resource.rs](../resource.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
