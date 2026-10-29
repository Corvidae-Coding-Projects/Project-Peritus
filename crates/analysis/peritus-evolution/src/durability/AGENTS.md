# crates/analysis/peritus-evolution/src/durability — navigation

## Responsibility

C0 persistence for evolution campaigns and the production-harness pointer.

## Files in this folder

| File | Role |
| --- | --- |
| [activation.rs](activation.rs) | Atomic promotion/rollback commit with approve-once consumption. |
| [binding.rs](binding.rs) | Stable C0 identities and cross-record transition checks. |
| [campaign.rs](campaign.rs) | Atomic ordinary campaign event and checkpoint persistence. |
| [directive.rs](directive.rs) | Deterministic publication outbox directives for F0 artifacts. |
| [mod.rs](mod.rs) | C0 persistence for evolution campaigns and the production-harness pointer. |
| [pointer.rs](pointer.rs) | Atomic ordinary production-pointer event and checkpoint persistence. |
| [replay.rs](replay.rs) | Reducer-driven campaign and pointer reconstruction from C0 records. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [activation/](activation/AGENTS.md) | Implementation pieces for `activation.rs`: Atomic promotion/rollback commit with approve-once consumption. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
