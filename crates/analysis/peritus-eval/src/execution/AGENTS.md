# crates/analysis/peritus-eval/src/execution — navigation

## Responsibility

Isolated two-stage rollout execution contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [directive.rs](directive.rs) | Capability-separated candidate and evaluator directives. |
| [mod.rs](mod.rs) | Isolated two-stage rollout execution contracts. |
| [observation.rs](observation.rs) | Checked output, verdict, failure, and resource observations. |
| [port.rs](port.rs) | External execution port and checked two-stage driver. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
