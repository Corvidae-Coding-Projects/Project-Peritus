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

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
