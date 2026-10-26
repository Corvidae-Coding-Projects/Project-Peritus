# crates/analysis/peritus-debugger/src/runtime — navigation

## Responsibility

Narrow effect orchestration over existing C0/C5/C6 owners.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Canonical validated-report artifact finalization. |
| [mod.rs](mod.rs) | Narrow effect orchestration over existing C0/C5/C6 owners. |
| [model.rs](model.rs) | Commit-before-provider model attempt execution and bounded retry scheduling. |
| [publication.rs](publication.rs) | Provenance-checked evidence admission and fenced publication settlement. |
| [recovery.rs](recovery.rs) | Deterministic restart decisions from durable state and owner observations. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
