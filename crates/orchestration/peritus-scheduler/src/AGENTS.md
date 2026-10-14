# crates/orchestration/peritus-scheduler/src — navigation

## Responsibility

Durable deterministic bounded resource scheduling for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Immutable run/revision/capacity binding. |
| [canonical.rs](canonical.rs) | Domain-separated canonical scheduler hashing and stable tags. |
| [command.rs](command.rs) | Closed pure scheduler command vocabulary. |
| [durability.rs](durability.rs) | Atomic C0 persistence and checked replay loading for one scheduler aggregate. |
| [error.rs](error.rs) | Stable scheduler failure and recovery vocabulary. |
| [event.rs](event.rs) | Immutable scheduler events and successor transitions. |
| [identity.rs](identity.rs) | Scheduler-owned stable nonzero identities. |
| [lib.rs](lib.rs) | Durable deterministic bounded resource scheduling for Peritus. |
| [limits.rs](limits.rs) | Independently checked scheduler bounds. |
| [projection.rs](projection.rs) | Rebuildable read-only scheduler projections. |
| [reducer.rs](reducer.rs) | Pure deterministic scheduler reduction and exact replay. |
| [resource.rs](resource.rs) | Checked canonical resource dimensions and arithmetic. |
| [runtime.rs](runtime.rs) | Inert effect directives reconstructed from committed state. |
| [selection.rs](selection.rs) | Pure deterministic bounded-bypass selection. |
| [semantics.rs](semantics.rs) | Closed scheduler queue and recovery semantics. |
| [session.rs](session.rs) | Bounded single-owner reuse of a verified scheduler replay. |
| [state.rs](state.rs) | Complete authoritative scheduler state. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `dependencies_are_ready`, `reservations_fit`, `resource_sum`, `unique_dispatch_ownership`, `attempts_are_monotonic`. |
| [work.rs](work.rs) | Immutable work specifications and authoritative lifecycle records. |
| [worker.rs](worker.rs) | Worker descriptors, lifecycle, and durable reservations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Domain-separated canonical scheduler hashing and stable tags. |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Closed pure scheduler command vocabulary. |
| [durability/](durability/AGENTS.md) | Implementation pieces for `durability.rs`: Atomic C0 persistence and checked replay loading for one scheduler aggregate. |
| [event/](event/AGENTS.md) | Implementation pieces for `event.rs`: Immutable scheduler events and successor transitions. |
| [identity/](identity/AGENTS.md) | Implementation pieces for `identity.rs`: Scheduler-owned stable nonzero identities. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure deterministic scheduler reduction and exact replay. |
| [resource/](resource/AGENTS.md) | Implementation pieces for `resource.rs`: Checked canonical resource dimensions and arithmetic. |
| [selection/](selection/AGENTS.md) | Implementation pieces for `selection.rs`: Pure deterministic bounded-bypass selection. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete authoritative scheduler state. |
| [verified/](verified/AGENTS.md) | Implementation pieces for `verified.rs`: Executable invariant/refinement predicates for the local domain. Entry names: `dependencies_are_ready`, `reservations_fit`, `resource_sum`, `unique_dispatch_ownership`, `attempts_are_monotonic`. |
| [wire/](wire/AGENTS.md) | D3-owned typed B3 codecs for reserved families 70, 71, and 72. |
| [work/](work/AGENTS.md) | Implementation pieces for `work.rs`: Immutable work specifications and authoritative lifecycle records. |
| [worker/](worker/AGENTS.md) | Implementation pieces for `worker.rs`: Worker descriptors, lifecycle, and durable reservations. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
