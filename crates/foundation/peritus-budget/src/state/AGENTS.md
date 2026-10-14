# crates/foundation/peritus-budget/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Privately represented ledger state, checked snapshots, and transition receipts.

## Files in this folder

| File | Role |
| --- | --- |
| [queries.rs](queries.rs) | Checked ledger queries and replay validation contracts. |
| [receipt.rs](receipt.rs) | Non-authorizing transition receipts and accepted value-in/value-out plans. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
