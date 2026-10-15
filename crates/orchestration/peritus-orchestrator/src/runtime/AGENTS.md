# crates/orchestration/peritus-orchestrator/src/runtime — navigation

## Responsibility

One-step commit-before-effect runtime shell and crash recovery classification.

## Files in this folder

| File | Role |
| --- | --- |
| [driver.rs](driver.rs) | One-transition driver preserving commit-before-effect ordering. |
| [mod.rs](mod.rs) | One-step commit-before-effect runtime shell and crash recovery classification. |
| [ports.rs](ports.rs) | Narrow effect and observation ports; no concrete provider or workspace dependencies. |
| [recovery.rs](recovery.rs) | Deterministic classification of one pending directive after restart. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
