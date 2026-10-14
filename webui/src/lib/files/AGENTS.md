# webui/src/lib/files — navigation

## Responsibility

Browser file draft/content lifecycle and protected local editing state.

## Files in this folder

| File | Role |
| --- | --- |
| [drafts.svelte.ts](drafts.svelte.ts) | Protected editor drafts, dirty-state preservation, and file lifecycle coordination. |
| [text.ts](text.ts) | Browser client module for text. Exports `TextSnapshot`, `textFormat`, `pageStarts`, `Change`, `difference`. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
