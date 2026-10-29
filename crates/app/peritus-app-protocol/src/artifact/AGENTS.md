# crates/app/peritus-app-protocol/src/artifact — navigation

## Responsibility

Bounded, transport-neutral artifact transfer contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Artifact-transfer rejection vocabulary. |
| [metadata.rs](metadata.rs) | Checked artifact metadata and exact transfer chunks. |
| [mod.rs](mod.rs) | Bounded, transport-neutral artifact transfer contracts. |
| [state.rs](state.rs) | Pure artifact-transfer state machine. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `chunk_is_contiguous`, `completion_is_conserved`. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
