# crates/app/peritus-daemon/src/product_run/interaction/tool_activity — navigation

## Responsibility

Implementation pieces for `tool_activity.rs`: Bounded observations of actual tool calls, not model prose or raw provider events.

## Files in this folder

| File | Role |
| --- | --- |
| [display.rs](display.rs) | Human-readable command/argv, saved Markdown sources and file-change previews correlated with successful host results; display-only, never shell input. |

Owning module: [tool_activity.rs](../tool_activity.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
