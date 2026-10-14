# crates/app/peritus-daemon/src/session/request — navigation

## Responsibility

Implementation pieces for `request.rs`: Closed A3 request dispatch for one authenticated connection.

## Files in this folder

| File | Role |
| --- | --- |
| [attachment.rs](attachment.rs) | Register and attach terminal ownership before recording connection cleanup bindings. |
| [discovery.rs](discovery.rs) | Bounded connection-owned model reads cannot hold up controls, events, or other requests. |
| [media.rs](media.rs) | Session-bound explicit file/image previews and artifact-upload admission. |
| [response.rs](response.rs) | Stable public response/error mapping for authenticated request dispatch. |
| [workbench.rs](workbench.rs) | Mutation-readiness routing for authenticated Workbench commands. |

Owning module: [request.rs](../request.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
