# crates/orchestration/peritus-gates/src/product — navigation

## Responsibility

Exact-target production gate planning and evidence aggregation.

## Files in this folder

| File | Role |
| --- | --- |
| [commands.rs](commands.rs) | Exact commands for each supported project family. |
| [mod.rs](mod.rs) | Exact-target production gate planning and evidence aggregation. |
| [plan.rs](plan.rs) | Changed-path to explicit project-check planning. |
| [report.rs](report.rs) | Fail-closed aggregation of exact command observations. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
