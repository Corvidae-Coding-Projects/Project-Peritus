# crates/analysis/peritus-debugger/src/wire — navigation

## Responsibility

Canonical inert B3 schema-v1 families 82, 83, and 84.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Inert canonical family-82 debugger command frames. |
| [event.rs](event.rs) | Inert canonical family-83 debugger event frames. |
| [mod.rs](mod.rs) | Canonical inert B3 schema-v1 families 82, 83, and 84. |
| [scalar.rs](scalar.rs) | Exact primitive readers shared by E2 wire families. |
| [semantic.rs](semantic.rs) | Strict schema-v1 command/event semantic payload codec. |
| [state.rs](state.rs) | Canonical family-84 complete debugger checkpoints. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
