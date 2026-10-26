# crates/app/peritus-web/src — navigation

## Responsibility

Loopback HTTP gateway, browser workspace/session service, CLI PTY console bridge, static assets, and daemon-facing routes.

## Files in this folder

| File | Role |
| --- | --- |
| [api.rs](api.rs) | Authenticated same-origin HTTP routes; all mutation sources share one dispatcher. Entry names: `router`, `protect`, `bootstrap`, `QueryArgs`, `query`. |
| [config.rs](config.rs) | Explicit gateway paths and user-editable TOML configuration. |
| [consoles.rs](consoles.rs) | Discoverable retained consoles and session-bound interactive CLI entry. |
| [daemon.rs](daemon.rs) | Native conversation adapter over the existing negotiated application client. |
| [error.rs](error.rs) | Actionable JSON failures, shared by the local HTTP boundary. |
| [files.rs](files.rs) | Bounded directory pages and canonical project-confined file access. |
| [git.rs](git.rs) | Git operations use argument vectors, exact pathspecs, and an explicit repository binding. |
| [main.rs](main.rs) | Local-only Peritus browser gateway. The daemon owns all agent execution. Entry names: `main`. |
| [operations.rs](operations.rs) | Read-only reconciliation and explicit human acknowledgement of unprovable outcomes. |
| [processes.rs](processes.rs) | Browser command observers reuse the C4/C2 owner, durable identity, and full output spools. |
| [server.rs](server.rs) | Gateway startup, local listener, and graceful shutdown. |
| [sessions.rs](sessions.rs) | Durable browser selections and exact daemon-run adoption. |
| [state.rs](state.rs) | Durable browser workspace and original operation outcomes. |
| [terminal.rs](terminal.rs) | Durable CLI console observers over the existing owned process service. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [api/](api/AGENTS.md) | Implementation pieces for `api.rs`: Authenticated same-origin HTTP routes; all mutation sources share one dispatcher. Entry names: `router`, `protect`, `bootstrap`, `QueryArgs`, `query`. |
| [daemon/](daemon/AGENTS.md) | Implementation pieces for `daemon.rs`: Native conversation adapter over the existing negotiated application client. |
| [files/](files/AGENTS.md) | Implementation pieces for `files.rs`: Bounded directory pages and canonical project-confined file access. |
| [git/](git/AGENTS.md) | Implementation pieces for `git.rs`: Git operations use argument vectors, exact pathspecs, and an explicit repository binding. |

## Working in this area

Start at [`main.rs`](main.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
