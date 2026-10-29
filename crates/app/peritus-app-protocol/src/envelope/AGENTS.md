# crates/app/peritus-app-protocol/src/envelope — navigation

## Responsibility

Typed post-negotiation application envelopes.

## Files in this folder

| File | Role |
| --- | --- |
| [context.rs](context.rs) | Exact negotiated relationship and durable-session binding. |
| [control.rs](control.rs) | Closed typed application control envelope. |
| [event.rs](event.rs) | Closed typed application event envelope. |
| [mod.rs](mod.rs) | Typed post-negotiation application envelopes. |
| [request.rs](request.rs) | Closed typed application request envelope. |
| [response.rs](response.rs) | Closed typed application response envelope. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
