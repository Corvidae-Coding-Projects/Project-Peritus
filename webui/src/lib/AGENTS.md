# webui/src/lib — navigation

## Responsibility

Browser workspace/session state, API bindings, commands, file drafts, operations, and reusable presentation components.

## Files in this folder

| File | Role |
| --- | --- |
| [api.ts](api.ts) | Typed browser HTTP calls to the Rust gateway. |
| [conversations.ts](conversations.ts) | Conversation selection/state helpers used by the workspace coordinator. |
| [modes.ts](modes.ts) | Shared Chat/Plan/Review/Build labels and behavior descriptions. |
| [operations.svelte.ts](operations.svelte.ts) | Reactive pending-operation/recovery tracking and replay controls. |
| [types.ts](types.ts) | Browser workspace, session, run, provider, file, and operation data shapes. |
| [workspace.svelte.ts](workspace.svelte.ts) | Reactive browser workspace/session coordinator, daemon/API polling, command dispatch, drafts, provider choices, and persistence. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [commands/](commands/AGENTS.md) | Browser command catalogue, slash-command routing, shortcut handling, and Git command integration. |
| [components/](components/AGENTS.md) | Reusable Svelte presentation components for navigation, conversations, files, dialogs, recovery, and the Nixie visual system. |
| [files/](files/AGENTS.md) | Browser file draft/content lifecycle and protected local editing state. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
