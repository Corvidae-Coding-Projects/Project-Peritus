# crates/app/peritus-daemon/src/outbox/qualification/gate — navigation

## Responsibility

Implementation pieces for `gate.rs`: Real D1 gate-state recovery on both sides of its atomic C0 commit.

## Files in this folder

| File | Role |
| --- | --- |
| [fixture.rs](fixture.rs) | Deterministic production D1 plan used by the gate commit crash probe. |

Owning module: [gate.rs](../gate.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
