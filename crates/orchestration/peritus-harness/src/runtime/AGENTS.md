# crates/orchestration/peritus-harness/src/runtime — navigation

## Responsibility

Commit-before-effect runtime driver and effect ports.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Read-only bridge from the stable shared revision tuple to exact E1 revision identity. |
| [driver.rs](driver.rs) | Commit-before-effect E1 runtime driver. |
| [mod.rs](mod.rs) | Commit-before-effect runtime driver and effect ports. |
| [ports.rs](ports.rs) | Narrow verified artifact-content port used by the effect shell. |
| [types.rs](types.rs) | Runtime type-state evidence, outcomes, and typed failures. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
