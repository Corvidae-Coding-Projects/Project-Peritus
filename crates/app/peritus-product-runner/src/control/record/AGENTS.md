# crates/app/peritus-product-runner/src/control/record — navigation

## Responsibility

Implementation pieces for `record.rs`: Revisioned conversation metadata and exact idempotent intents.

## Files in this folder

| File | Role |
| --- | --- |
| [codec.rs](codec.rs) | Bounded durable control encoding shared by records, operations and receipts. |
| [intent.rs](intent.rs) | Closed durable control intents. |
| [projection.rs](projection.rs) | Conversation aggregate inspection, encoding, and invariant validation. |
| [receipt.rs](receipt.rs) | Exact original-receipt resolution; this never applies a current-state transition. |
| [replay.rs](replay.rs) | Revision fencing derived from exact history, never from a mutable cached counter. |
| [seed.rs](seed.rs) | Immutable historical context only. No execution, goal qualification, or effect receipts. |
| [transition.rs](transition.rs) | Pure expected-revision successor planning and receipt binding. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [transition/](transition/AGENTS.md) | Implementation pieces for `transition.rs`: Pure expected-revision successor planning and receipt binding. |

Owning module: [record.rs](../record.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
