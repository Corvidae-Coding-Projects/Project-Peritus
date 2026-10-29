# crates/observe/peritus-telemetry/src/export — navigation

## Responsibility

Stable idempotent exporter batches, acknowledgements, and shutdown pumping.

## Files in this folder

| File | Role |
| --- | --- |
| [encoding.rs](encoding.rs) | Canonical redaction-safe hashing representation for export records and batches. |
| [mod.rs](mod.rs) | Stable idempotent exporter batches, acknowledgements, and shutdown pumping. |
| [pump.rs](pump.rs) | Bounded export and shutdown state machine. |
| [types.rs](types.rs) | Export identities, records, batches, acknowledgements, and adapter contract. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
