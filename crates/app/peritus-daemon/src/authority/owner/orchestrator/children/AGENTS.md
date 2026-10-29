# crates/app/peritus-daemon/src/authority/owner/orchestrator/children — navigation

## Responsibility

Implementation pieces for `children.rs`: Durable child admission before exact E0/C0 claim settlement.

## Files in this folder

| File | Role |
| --- | --- |
| [collaboration.rs](collaboration.rs) | Native D3 collaboration lifecycle, cancellation, and finalization admission. |
| [gates_review.rs](gates_review.rs) | Native D1/D2 pause and resume admission. |
| [scheduler.rs](scheduler.rs) | Native D3 scheduler pause and resume admission. |

Owning module: [children.rs](../children.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
