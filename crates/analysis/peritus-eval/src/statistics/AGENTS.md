# crates/analysis/peritus-eval/src/statistics — navigation

## Responsibility

Portable bounded E3 statistical calculations.

## Files in this folder

| File | Role |
| --- | --- |
| [distribution.rs](distribution.rs) | Checked integer distribution summaries with nearest-rank percentiles. |
| [interval.rs](interval.rs) | Frozen 95% Wilson intervals for raw binomial proportions. |
| [mod.rs](mod.rs) | Portable bounded E3 statistical calculations. |
| [paired.rs](paired.rs) | Task-cluster paired comparison, bootstrap interval, and task-level sign diagnostic. |
| [pass_at_k.rs](pass_at_k.rs) | Deterministic fixed-point pass@k estimator. |
| [stability.rs](stability.rs) | Per-task ordered rollout stability analysis. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
