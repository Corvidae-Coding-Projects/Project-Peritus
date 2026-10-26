# crates/analysis/peritus-debugger/src/model — navigation

## Responsibility

Provider-neutral, tool-free optional model analysis.

## Files in this folder

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Provider-neutral, tool-free optional model analysis. |
| [plan.rs](plan.rs) | Frozen C6-to-C5 request mapping and complete model plan identity. |
| [proposal.rs](proposal.rs) | Closed JSON proposal decoding and manifest-contained validation. |
| [runner.rs](runner.rs) | Bounded pull-based provider execution and strict terminal reduction. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
