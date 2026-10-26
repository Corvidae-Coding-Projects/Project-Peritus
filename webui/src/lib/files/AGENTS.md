# webui/src/lib/files — navigation

## Responsibility

Browser file draft/content lifecycle and protected local editing state.

## Files in this folder

| File | Role |
| --- | --- |
| [drafts.svelte.ts](drafts.svelte.ts) | Protected editor drafts, dirty-state preservation, and file lifecycle coordination. |
| [text.ts](text.ts) | Browser client module for text. Exports `TextSnapshot`, `textFormat`, `pageStarts`, `Change`, `difference`. |

## Working in this area

Start with the nearest component or state entry point, then trace UI state through the typed API boundary to the Rust gateway and daemon owner before changing behavior. Keep presentation-only state separate from durable run authority, and update the local component or file route when the browser surface moves.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
