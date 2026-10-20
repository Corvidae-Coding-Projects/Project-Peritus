# crates/app/peritus-daemon/src/product_run/persistence — navigation

## Responsibility

Implementation pieces for `persistence.rs`: Durable product-run snapshots and restart recovery.

## Files in this folder

| File | Role |
| --- | --- |
| [index.rs](index.rs) | Disposable metadata indexes; full authority and continuation validation is deferred until selection. |
| [continuation.rs](continuation.rs) | Digest-addressed continuation bytes published before their small atomic run-record pointer. |
| [deliverable.rs](deliverable.rs) | Durable candidate projection, independent from interactive mode. |
| [interaction.rs](interaction.rs) | Strictly checked durable conversation-mode extension. |
| [preview.rs](preview.rs) | Persistent preview reconstruction and restart-safe process-state projection. |
| [progress.rs](progress.rs) | Conversion between persisted and live product-run progress. |
| [record.rs](record.rs) | Conversion between live product-run ownership and persisted records. |
| [settlement.rs](settlement.rs) | Stable JSON projection of verified candidate checkpoints and settlements. |
| [types.rs](types.rs) | Existing persisted JSON fields, separated from I/O and recovery logic. |
| [workbench.rs](workbench.rs) | Fenced-generation recovery; opening state never retries an admitted or staged execution. |

Owning module: [persistence.rs](../persistence.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
