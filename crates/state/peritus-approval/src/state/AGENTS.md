# crates/state/peritus-approval/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Move-only approval aggregate and total checked reducers.

## Files in this folder

| File | Role |
| --- | --- |
| [exact.rs](exact.rs) | Shared exact value relations for public reducer contracts. |
| [independence.rs](independence.rs) | Exact approver-independence predicates and executable checks. |
| [lifecycle.rs](lifecycle.rs) | Pending cancellation and exact expiry transitions. |
| [lifecycle_specification.rs](lifecycle_specification.rs) | Closed total exact relations for cancellation and expiry reducers. |
| [resolve_helpers.rs](resolve_helpers.rs) | Small exact helpers shared by authenticated resolution and lifecycle rejection. |
| [resolve_specification.rs](resolve_specification.rs) | Closed total exact relation for authenticated approval resolution. |
| [specification.rs](specification.rs) | Closed executable-state projection used by reducer refinement contracts. |
| [types.rs](types.rs) | Private representation and public unprivileged lifecycle projections. |
| [use_reducers.rs](use_reducers.rs) | Exact single-use and amendment-use reducers. |
| [use_specification.rs](use_specification.rs) | Closed total exact relations for approval-use reducers. |
| [validation.rs](validation.rs) | Exact reducer-side revalidation of checked authentication observations. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
