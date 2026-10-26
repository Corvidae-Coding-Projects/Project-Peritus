# webui/scripts — navigation

## Responsibility

Frontend inspection utility for observing the real browser interface and retaining screenshots/interaction observations.

## Files in this folder

| File | Role |
| --- | --- |
| [inspect.mjs](inspect.mjs) | Browser client module for inspect. |

## Working in this area

Start with the nearest component or state entry point, then trace UI state through the typed API boundary to the Rust gateway and daemon owner before changing behavior. Keep presentation-only state separate from durable run authority, and update the local component or file route when the browser surface moves.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
