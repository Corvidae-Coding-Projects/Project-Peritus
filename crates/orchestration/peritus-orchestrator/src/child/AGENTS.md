# crates/orchestration/peritus-orchestrator/src/child — navigation

## Responsibility

Implementation pieces for `child.rs`: Checked authority-free observations of E0 child aggregate truth.

## Files in this folder

| File | Role |
| --- | --- |
| [agent.rs](agent.rs) | D0 writer and fixer observations bound to exact E0/D3 handoffs. |
| [d3.rs](d3.rs) | D3 scheduler and collaboration terminal observations. |
| [gates.rs](gates.rs) | D1 gate observations with failure-cause conservation. |
| [kernel.rs](kernel.rs) | Exact B0 acceptance event observations. |
| [review.rs](review.rs) | D2 review observations with quorum and finding conservation. |

Owning module: [child.rs](../child.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
