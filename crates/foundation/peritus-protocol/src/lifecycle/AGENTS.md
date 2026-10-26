# crates/foundation/peritus-protocol/src/lifecycle — navigation

## Responsibility

Version-one lifecycle protocol families.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical lifecycle command request. |
| [envelope.rs](envelope.rs) | Canonical reducer invocation envelope. |
| [error.rs](error.rs) | Canonical inert reducer failure record. |
| [event.rs](event.rs) | Canonical inert lifecycle event record. |
| [mod.rs](mod.rs) | Version-one lifecycle protocol families. |
| [phase.rs](phase.rs) | Canonical lifecycle phase vocabulary. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
