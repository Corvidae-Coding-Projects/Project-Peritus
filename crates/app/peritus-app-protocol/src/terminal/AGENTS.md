# crates/app/peritus-app-protocol/src/terminal — navigation

## Responsibility

Bounded terminal attachment records and ordering state.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Terminal-state rejection vocabulary. |
| [messages.rs](messages.rs) | Exact attachment-bound terminal input, output, resize, and terminal messages. |
| [mod.rs](mod.rs) | Bounded terminal attachment records and ordering state. |
| [state.rs](state.rs) | Pure terminal attachment ordering state machine. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `output_is_contiguous`, `output_position_is_valid`. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
