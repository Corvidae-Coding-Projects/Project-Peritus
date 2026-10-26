# crates/app/peritus-daemon/src/product_run/interaction — navigation

## Responsibility

Live conversation/control/provider selection projected into role execution; this is the connection between new user input and an ongoing run.

## Files in this folder

| File | Role |
| --- | --- |
| [inputs.rs](inputs.rs) | Durable public projection of accepted ledger input, separate from inference admission. |
| [live.rs](live.rs) | Live D0 input, provider selection, admission and typed activity adapter. |
| [models.rs](models.rs) | Authorized, concurrent-update-fenced and durable future-turn model/provider selection, independent of input admission or execution restart. |
| [narration.rs](narration.rs) | Public host observations. Never infer success, reveal arguments, or impersonate model reasoning. |
| [presentation.rs](presentation.rs) | Human-readable projections of completed pipeline response envelopes. |
| [tool_activity.rs](tool_activity.rs) | Bounded observations of actual tool calls, not model prose or raw provider events. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [live/](live/AGENTS.md) | Implementation pieces for `live.rs`: Live D0 input, provider selection, admission and typed activity adapter. |
| [tool_activity/](tool_activity/AGENTS.md) | Implementation pieces for `tool_activity.rs`: Bounded observations of actual tool calls, not model prose or raw provider events. |

Owning module: [interaction.rs](../interaction.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
