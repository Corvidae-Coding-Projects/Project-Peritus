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

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
