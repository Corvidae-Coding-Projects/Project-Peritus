# crates/orchestration/peritus-collaboration/src — navigation

## Responsibility

Durable deterministic causal collaboration for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Immutable run, scheduler, revision, root, and limit binding. |
| [canonical.rs](canonical.rs) | Canonical semantic digests independent of B3 frame headers. |
| [command.rs](command.rs) | Closed pure D3 collaboration command vocabulary. |
| [durability.rs](durability.rs) | Atomic C0 persistence and checked replay loading for collaboration aggregates. |
| [error.rs](error.rs) | Stable D3 collaboration errors and caller recovery actions. |
| [event.rs](event.rs) | Immutable collaboration facts and complete successor transitions. |
| [identity.rs](identity.rs) | Collaboration-owned checked identities. |
| [join.rs](join.rs) | Closed join policy and exact artifact handoff evidence. |
| [lib.rs](lib.rs) | Durable deterministic causal collaboration for Peritus. |
| [limits.rs](limits.rs) | Independently checked collaboration bounds. |
| [message.rs](message.rs) | Bounded inert collaboration messages and delivery state. |
| [projection.rs](projection.rs) | Rebuildable read-only projections over authoritative collaboration state. |
| [reducer.rs](reducer.rs) | Pure deterministic collaboration transition and exact replay. |
| [replay.rs](replay.rs) | Checked durable replay observations and exact checkpoint comparison. |
| [state.rs](state.rs) | Complete authoritative D3 collaboration state. |
| [task.rs](task.rs) | Task assignment, ownership, reservation, cancellation, and terminal records. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Canonical semantic digests independent of B3 frame headers. |
| [durability/](durability/AGENTS.md) | Implementation pieces for `durability.rs`: Atomic C0 persistence and checked replay loading for collaboration aggregates. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure deterministic collaboration transition and exact replay. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete authoritative D3 collaboration state. |
| [verified/](verified/AGENTS.md) | Implementation pieces for `verified.rs`: Executable invariant/refinement predicates for the local domain. |
| [wire/](wire/AGENTS.md) | Collaboration-owned typed B3 codecs for canonical families 73, 74, and 75. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
