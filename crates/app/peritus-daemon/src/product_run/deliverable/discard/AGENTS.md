# crates/app/peritus-daemon/src/product_run/deliverable/discard — navigation

## Responsibility

Implementation pieces for `discard.rs`: Durable acknowledgement of completed source restoration, independent of run projection saves.

## Files in this folder

| File | Role |
| --- | --- |
| [reservation.rs](reservation.rs) | Reserve a writable, run-bound acknowledgement before restoration changes files. |
| [transaction.rs](transaction.rs) | A bound pending discard keeps its original authority across partial workspace changes. |

Owning module: [discard.rs](../discard.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
