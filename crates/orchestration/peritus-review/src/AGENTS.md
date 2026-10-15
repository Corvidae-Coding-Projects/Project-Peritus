# crates/orchestration/peritus-review/src — navigation

## Responsibility

Durable deterministic review cycles and finding lifecycle for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Immutable B2 contract and exact candidate-revision binding. |
| [canonical.rs](canonical.rs) | Domain-canonical D2 hashing. |
| [command.rs](command.rs) | Closed pure D2 command vocabulary. |
| [disposition.rs](disposition.rs) | Append-only finding response and disposition records. |
| [durability.rs](durability.rs) | Atomic C0 persistence and checked replay loading for one D2 run aggregate. |
| [error.rs](error.rs) | Stable D2 error and recovery vocabulary. |
| [event.rs](event.rs) | Immutable D2 facts and pure successor transitions. |
| [finding.rs](finding.rs) | Checked structured review submissions and complete finding history. |
| [lib.rs](lib.rs) | Durable deterministic review cycles and finding lifecycle for Peritus. |
| [limits.rs](limits.rs) | Independently checked production bounds for D2 state and payloads. |
| [observation.rs](observation.rs) | Canonical current B2 quality observations derived without authority widening. |
| [oscillation.rs](oscillation.rs) | Deterministic repeated-finding, severity-trend, disagreement, and exhaustion reporting. |
| [projection.rs](projection.rs) | Rebuildable read-only query projections over authoritative D2 state. |
| [quorum.rs](quorum.rs) | Independent named review-quorum dimensions. |
| [reconciliation.rs](reconciliation.rs) | Provenance-preserving duplicate and confirmed-supersession reconciliation. |
| [reducer.rs](reducer.rs) | Pure deterministic D2 transition, terminal truth, and replay. |
| [replay.rs](replay.rs) | Checked durable replay observations and semantic checkpoint comparison. |
| [reviewer.rs](reviewer.rs) | Immutable reviewer assignments and one-shot cycle submissions. |
| [state.rs](state.rs) | Complete authoritative D2 review-run state. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `DispositionModel`. |
| [waiver.rs](waiver.rs) | Consumption-only wrapper for external B2 waiver observations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Domain-canonical D2 hashing. |
| [durability/](durability/AGENTS.md) | Implementation pieces for `durability.rs`: Atomic C0 persistence and checked replay loading for one D2 run aggregate. |
| [finding/](finding/AGENTS.md) | Implementation pieces for `finding.rs`: Checked structured review submissions and complete finding history. |
| [product/](product/AGENTS.md) | Production-facing typed review and finding conservation. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure deterministic D2 transition, terminal truth, and replay. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete authoritative D2 review-run state. |
| [verified/](verified/AGENTS.md) | Implementation pieces for `verified.rs`: Executable invariant/refinement predicates for the local domain. Entry names: `DispositionModel`. |
| [wire/](wire/AGENTS.md) | D2-owned typed B3 codecs for reserved families 53, 54, and 55. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
