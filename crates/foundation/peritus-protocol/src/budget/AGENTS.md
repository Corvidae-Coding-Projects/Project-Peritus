# crates/foundation/peritus-protocol/src/budget — navigation

## Responsibility

Version-one budget protocol families.

## Files in this folder

| File | Role |
| --- | --- |
| [amounts.rs](amounts.rs) | Canonical fixed-dimensional budget values. |
| [command.rs](command.rs) | Canonical budget reducer requests and unprivileged evidence claims. |
| [error.rs](error.rs) | Canonical inert budget failures. |
| [mod.rs](mod.rs) | Version-one budget protocol families. |
| [receipt.rs](receipt.rs) | Canonical inert budget transition receipts. |
| [snapshot.rs](snapshot.rs) | Canonical inert budget snapshots. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
