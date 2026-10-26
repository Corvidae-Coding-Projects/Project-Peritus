# crates/orchestration/peritus-scheduler/src/wire — navigation

## Responsibility

D3-owned typed B3 codecs for reserved families 70, 71, and 72.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical family-70 schema-v2 scheduler command frame. |
| [domain.rs](domain.rs) | Wire encoding for worker, work, and reservation domain values. |
| [event.rs](event.rs) | Canonical family-71 schema-v2 scheduler event frame. |
| [mod.rs](mod.rs) | D3-owned typed B3 codecs for reserved families 70, 71, and 72. |
| [records.rs](records.rs) | Wire encoding for mutable records and terminal summaries. |
| [state.rs](state.rs) | Canonical family-72 schema-v2 complete scheduler checkpoint. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
