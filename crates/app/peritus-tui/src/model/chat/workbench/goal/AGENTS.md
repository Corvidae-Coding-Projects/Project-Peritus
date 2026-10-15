# crates/app/peritus-tui/src/model/chat/workbench/goal — navigation

## Responsibility

Implementation pieces for `goal.rs`: Explicit persistent-goal confirmation and revision-fenced pause, resume, and usage.

## Files in this folder

| File | Role |
| --- | --- |
| [draft.rs](draft.rs) | Explicit local goal drafting and reviewed confirmation. |
| [refresh.rs](refresh.rs) | Refresh a stale goal panel before submitting the user's unchanged control command. |

Owning module: [goal.rs](../goal.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
