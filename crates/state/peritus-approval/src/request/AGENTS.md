# crates/state/peritus-approval/src/request — navigation

## Responsibility

Implementation pieces for `request.rs`: Canonical bounded approval requests and participant facts.

## Files in this folder

| File | Role |
| --- | --- |
| [participant.rs](participant.rs) | Canonical bounded participant-set construction and membership. |
| [projection.rs](projection.rs) | Immutable challenge-time projections for bounded rendering. |
| [specification.rs](specification.rs) | Exact participant-set projections used by reducer contracts. |

Owning module: [request.rs](../request.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
