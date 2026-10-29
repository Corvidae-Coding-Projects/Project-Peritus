# crates/foundation/peritus-protocol/src/agent — navigation

## Responsibility

Stable inert records used to persist and replay one D0 agent turn.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical inert D0 command records. |
| [event.rs](event.rs) | Canonical inert D0 event records. |
| [mod.rs](mod.rs) | Stable inert records used to persist and replay one D0 agent turn. |
| [phase.rs](phase.rs) | Closed D0 phase vocabulary carried by inert protocol records. |
| [state.rs](state.rs) | Canonical inert D0 state checkpoints. |
| [wire.rs](wire.rs) | Shared canonical wire helpers for the three D0 families. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
