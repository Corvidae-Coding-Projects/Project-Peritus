# crates/app/peritus-tui/src/model/protocol — navigation

## Responsibility

Implementation pieces for `protocol.rs`: Protocol message admission and daemon-derived state projection.

## Files in this folder

| File | Role |
| --- | --- |
| [response.rs](response.rs) | Request-correlated response projection and retained draft recovery. |
| [timeout.rs](timeout.rs) | Read-only timeouts are local failures; uncertain effects still require reconciliation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [response/](response/AGENTS.md) | Implementation pieces for `response.rs`: Request-correlated response projection and retained draft recovery. |

Owning module: [protocol.rs](../protocol.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
