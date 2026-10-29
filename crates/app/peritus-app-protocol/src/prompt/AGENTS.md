# crates/app/peritus-app-protocol/src/prompt — navigation

## Responsibility

Correlated, revision-fresh prompt values and pure admission state.

## Files in this folder

| File | Role |
| --- | --- |
| [approval.rs](approval.rs) | Opaque bounded B1 approval authority frames carried by A3. |
| [binding.rs](binding.rs) | Immutable prompt correlation and bounded choice/constraint definitions. |
| [error.rs](error.rs) | Prompt-admission rejection vocabulary. |
| [mod.rs](mod.rs) | Correlated, revision-fresh prompt values and pure admission state. |
| [response.rs](response.rs) | Closed unprivileged prompt answer values. |
| [state.rs](state.rs) | Pure one-answer prompt admission state. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
