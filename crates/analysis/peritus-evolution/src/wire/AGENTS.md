# crates/analysis/peritus-evolution/src/wire — navigation

## Responsibility

Closed canonical schema-v1 F0 wire families.

## Files in this folder

| File | Role |
| --- | --- |
| [campaign.rs](campaign.rs) | Canonical campaign command, event, and complete-state families 88-90. |
| [mod.rs](mod.rs) | Closed canonical schema-v1 F0 wire families. |
| [pointer.rs](pointer.rs) | Canonical production-pointer command, event, and complete-state families 91-93. |
| [scalar.rs](scalar.rs) | Exact primitive and cross-slice identity codecs shared by F0 wire families. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [semantic/](semantic/AGENTS.md) | Strict production-limited semantic codecs shared by all six F0 frame families. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
