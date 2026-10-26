# crates/analysis/peritus-eval/src/dataset — navigation

## Responsibility

Immutable dataset manifests and candidate/evaluator isolation.

## Files in this folder

| File | Role |
| --- | --- |
| [manifest.rs](manifest.rs) | Complete canonical dataset validation and digest binding. |
| [mod.rs](mod.rs) | Immutable dataset manifests and candidate/evaluator isolation. |
| [partition.rs](partition.rs) | Closed evaluation-corpus partitions. |
| [task.rs](task.rs) | Checked task descriptors with disjoint candidate and evaluator inputs. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
