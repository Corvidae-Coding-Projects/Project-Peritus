# crates/foundation/peritus-quality-policy/src/model — navigation

## Responsibility

Implementation pieces for `model.rs`: Mathematical acceptance and freshness predicates.

## Files in this folder

| File | Role |
| --- | --- |
| [approvals.rs](approvals.rs) | Input-defined final and unexpected approval predicates. |
| [artifacts.rs](artifacts.rs) | Contract-derived artifact membership and exact-revision completeness. |
| [authority.rs](authority.rs) | Input-defined waiver authority, blocker resolution, and diagnostic coverage. |
| [gates.rs](gates.rs) | Contract-derived gate membership and exact-revision passing results. |
| [reviews.rs](reviews.rs) | Contract-derived review categories, quorum, and supplied independence facts. |

Owning module: [model.rs](../model.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
