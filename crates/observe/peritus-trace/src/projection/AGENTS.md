# crates/observe/peritus-trace/src/projection — navigation

## Responsibility

Pure causal trace projection over checked C0 records.

## Files in this folder

| File | Role |
| --- | --- |
| [encoding.rs](encoding.rs) | Deterministic projection payload and invariant encoding. |
| [fold.rs](fold.rs) | Exact span lifecycle and parent-transition fold rules. |
| [mod.rs](mod.rs) | Pure causal trace projection over checked C0 records. |
| [state.rs](state.rs) | Projection state values and observation application. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
