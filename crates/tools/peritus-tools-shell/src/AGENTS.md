# crates/tools/peritus-tools-shell/src — navigation

## Responsibility

Authorized structured argv and explicit script tool adapters.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Exact deterministic shell tool descriptors. |
| [dispatcher.rs](dispatcher.rs) | Router-permit-only shell dispatch through the restricted native C2 gateway. |
| [error.rs](error.rs) | Stable shell-tool errors. |
| [execution.rs](execution.rs) | Owned C2 process to C4 active-execution adapter. |
| [input.rs](input.rs) | Checked structured argv and separately classified script inputs. |
| [json_value.rs](json_value.rs) | Explicit JSON value assembly for ordinary tool projections. |
| [lib.rs](lib.rs) | Authorized structured argv and explicit script tool adapters. |
| [plan.rs](plan.rs) | Complete C2 execution-plan compiler inputs. |
| [render.rs](render.rs) | Control-safe independently bounded render windows. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [execution/](execution/AGENTS.md) | Implementation pieces for `execution.rs`: Owned C2 process to C4 active-execution adapter. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
