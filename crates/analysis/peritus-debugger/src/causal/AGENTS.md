# crates/analysis/peritus-debugger/src/causal — navigation

## Responsibility

Deterministic analyzer registry and bounded root-cause candidates.

## Files in this folder

| File | Role |
| --- | --- |
| [analyzer.rs](analyzer.rs) | Fixed deterministic analyzer registry. |
| [candidate.rs](candidate.rs) | Checked root-cause, ambiguity, alternative, and unsupported values. |
| [confidence.rs](confidence.rs) | Explainable bounded integer confidence calculations. |
| [mod.rs](mod.rs) | Deterministic analyzer registry and bounded root-cause candidates. |
| [rules.rs](rules.rs) | Frozen typed mappings used by the deterministic analyzer registry. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
