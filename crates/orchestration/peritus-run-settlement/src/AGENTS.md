# crates/orchestration/peritus-run-settlement/src — navigation

## Responsibility

Candidate-checkpoint and terminal-settlement domain for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [cause.rs](cause.rs) | Typed reason a run reached settlement. |
| [checkpoint.rs](checkpoint.rs) | Checked candidate checkpoint with current qualification evidence. |
| [disposition.rs](disposition.rs) | User-visible terminal disposition derived by the settlement reducer. |
| [error.rs](error.rs) | Stable checked-construction and reducer failures. |
| [evidence.rs](evidence.rs) | Candidate-bound qualification evidence. |
| [identity.rs](identity.rs) | Exact identity of one observed workspace candidate. |
| [lib.rs](lib.rs) | Candidate-checkpoint and terminal-settlement domain for Peritus. |
| [observation.rs](observation.rs) | One authority for evidence freshness and the qualification it currently supports. |
| [reducer.rs](reducer.rs) | Pure monotonic checkpoint and exactly-once settlement reducer. |
| [settlement.rs](settlement.rs) | Immutable terminal settlement derived from the latest candidate checkpoint. |
| [stage.rs](stage.rs) | Currently supported qualification stage for one exact candidate. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `acceptance_allowed`, `checkpoint_advances`, `evidence_is_current`, `terminal_transition_allowed`. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
