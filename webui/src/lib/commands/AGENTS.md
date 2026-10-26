# webui/src/lib/commands — navigation

## Responsibility

Browser command catalogue, slash-command routing, shortcut handling, and Git command integration.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.ts](catalog.ts) | Browser command metadata, aliases, and shortcut matching/display. |
| [git.ts](git.ts) | Browser Git command integration and panel operations. |
| [slash.ts](slash.ts) | Typed parsing/routing of slash commands entered in the browser composer. |

## Working in this area

Start with the nearest component or state entry point, then trace UI state through the typed API boundary to the Rust gateway and daemon owner before changing behavior. Keep presentation-only state separate from durable run authority, and update the local component or file route when the browser surface moves.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
