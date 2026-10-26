# crates/app/peritus-app-protocol/src/daemon — navigation

## Responsibility

Honest daemon readiness, heartbeat, and shutdown observations.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Daemon-control rejection vocabulary. |
| [heartbeat.rs](heartbeat.rs) | Monotonic heartbeat nonce observations without wall-clock semantics. |
| [mod.rs](mod.rs) | Honest daemon readiness, heartbeat, and shutdown observations. |
| [shutdown.rs](shutdown.rs) | Explicit shutdown request, acceptance, draining, and completion truth. |
| [status.rs](status.rs) | Distinct readiness and bounded diagnostic status values. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
