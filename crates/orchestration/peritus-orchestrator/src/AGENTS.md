# crates/orchestration/peritus-orchestrator/src — navigation

## Responsibility

Durable deterministic E0 delivery orchestration for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [acceptance.rs](acceptance.rs) | B2-derived certificate data that cannot itself accept a B0 run. |
| [binding.rs](binding.rs) | Immutable contract, run, child-domain, revision, and limit binding. |
| [candidate.rs](candidate.rs) | Immutable exact candidate, workspace, artifact, and producer binding. |
| [canonical.rs](canonical.rs) | Canonical E0 digests and protocol tag vocabulary. |
| [child.rs](child.rs) | Checked authority-free observations of E0 child aggregate truth. |
| [command.rs](command.rs) | Closed fenced E0 command vocabulary. |
| [directive.rs](directive.rs) | Stable commit-before-effect directives and delivery state. |
| [durability.rs](durability.rs) | Atomic C0 persistence, outbox installation, and checked E0 replay loading. |
| [error.rs](error.rs) | Stable typed E0 failures and operator recovery actions. |
| [event.rs](event.rs) | Immutable E0 facts and pure successor transitions. |
| [handoff.rs](handoff.rs) | Exact bounded role handoffs without free-form hidden reasoning. |
| [identity.rs](identity.rs) | Stable nonzero E0 aggregate and handoff identities. |
| [lib.rs](lib.rs) | Durable deterministic E0 delivery orchestration for Peritus. |
| [limits.rs](limits.rs) | Independent immutable E0 completion and retention bounds. |
| [ownership.rs](ownership.rs) | Immutable exact B1 and C6 role ownership agreement. |
| [phase.rs](phase.rs) | Closed E0 lifecycle phases. |
| [product.rs](product.rs) | Production-facing E0 decision composition. |
| [projection.rs](projection.rs) | Stable read-only E0 lifecycle projection. |
| [reducer.rs](reducer.rs) | Pure deterministic E0 transitions and exact event replay. |
| [replay.rs](replay.rs) | Checked E0 replay bundle reconstructed from C0 records. |
| [state.rs](state.rs) | Complete authoritative replayable E0 aggregate state. |
| [terminal.rs](terminal.rs) | Truthful terminal classifications for the orchestration lifecycle. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `roles_are_separated`, `candidate_cycles_are_fresh`, `counters_are_bounded`, `terminal_is_truthful`, `cancellation_dominates`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [binding/](binding/AGENTS.md) | Implementation pieces for `binding.rs`: Immutable contract, run, child-domain, revision, and limit binding. |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Canonical E0 digests and protocol tag vocabulary. |
| [child/](child/AGENTS.md) | Implementation pieces for `child.rs`: Checked authority-free observations of E0 child aggregate truth. |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Closed fenced E0 command vocabulary. |
| [durability/](durability/AGENTS.md) | Implementation pieces for `durability.rs`: Atomic C0 persistence, outbox installation, and checked E0 replay loading. |
| [qualification/](qualification/AGENTS.md) | Deterministic production-lifecycle fixtures for external resilience qualification. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure deterministic E0 transitions and exact event replay. |
| [runtime/](runtime/AGENTS.md) | One-step commit-before-effect runtime shell and crash recovery classification. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete authoritative replayable E0 aggregate state. |
| [wire/](wire/AGENTS.md) | Orchestrator-owned canonical families 76, 77, and 78. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
