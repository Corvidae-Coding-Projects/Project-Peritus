# crates/analysis/peritus-evolution/src/selection — navigation

## Responsibility

Deny-wins eligibility and deterministic stable multi-objective selection.

## Files in this folder

| File | Role |
| --- | --- |
| [assessment.rs](assessment.rs) | Independent criterion results and stable objective vectors. |
| [decision.rs](decision.rs) | Immutable selected/no-eligible decision records. |
| [engine.rs](engine.rs) | Pure deny-wins assessment and stable lexicographic selection. |
| [mod.rs](mod.rs) | Deny-wins eligibility and deterministic stable multi-objective selection. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
