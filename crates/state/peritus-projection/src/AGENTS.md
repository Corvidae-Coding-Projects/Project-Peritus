# crates/state/peritus-projection/src — navigation

## Responsibility

Pure deterministic journal projections and durable shadow rebuilds for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [agent.rs](agent.rs) | Latest durable D0 inner-turn observations over canonical agent-event records. |
| [artifacts.rs](artifacts.rs) | Actual committed artifact-dependency catalog projection. |
| [authority.rs](authority.rs) | Authority catalog projection over canonical policy and acceptance records. |
| [budget.rs](budget.rs) | Budget projection over immutable snapshot and receipt families. |
| [catalog.rs](catalog.rs) | Durable generation observations and startup repair plans. |
| [checkpoint.rs](checkpoint.rs) | Versioned projection identities, schemas, and journal-bound checkpoints. |
| [encoding.rs](encoding.rs) | Small deterministic binary encoding helpers for projection payloads. |
| [error.rs](error.rs) | Stable projection failure and recovery vocabulary. |
| [evidence.rs](evidence.rs) | Evidence digest catalog projection with no effect on unrelated records. |
| [fold.rs](fold.rs) | Effect-free projection fold contracts. |
| [journal_catalog.rs](journal_catalog.rs) | Complete aggregate and family catalog projection. |
| [lib.rs](lib.rs) | Pure deterministic journal projections and durable shadow rebuilds for Peritus. |
| [lifecycle.rs](lifecycle.rs) | Lifecycle projection over canonical kernel-event records. |
| [rebuild.rs](rebuild.rs) | Pure shadow-generation rebuild preparation. |
| [replay.rs](replay.rs) | Checked replay from journal genesis. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `next_position`, `position_transition`, `sequence_transition`, `checkpoint_current`, `next_generation`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [sqlite/](sqlite/AGENTS.md) | Narrow `SQLite` adapter for shared-file projection generations. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
