# crates/analysis/peritus-eval/src/runtime — navigation

## Responsibility

Narrow effect orchestration over existing C0 owners.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Canonical evaluation-report artifact finalization and commit. |
| [mod.rs](mod.rs) | Narrow effect orchestration over existing C0 owners. |
| [publication.rs](publication.rs) | Provenance-checked evidence admission and publication settlement. |
| [recovery.rs](recovery.rs) | Deterministic crash-recovery decisions from durable observations. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
