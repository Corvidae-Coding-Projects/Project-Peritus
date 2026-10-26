# crates/app/peritus-app-protocol/src/error — navigation

## Responsibility

Stable application-protocol error vocabulary.

## Files in this folder

| File | Role |
| --- | --- |
| [code.rs](code.rs) | Stable machine-actionable application error codes. |
| [diagnostic.rs](diagnostic.rs) | Bounded human-readable application diagnostics. |
| [mod.rs](mod.rs) | Stable application-protocol error vocabulary. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
