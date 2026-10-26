# crates/app/peritus-daemon/src/ipc/windows — navigation

## Responsibility

Implementation pieces for `windows.rs`: Same-user Windows named-pipe endpoint and peer-token admission.

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [native/](native/AGENTS.md) | Win32 named-pipe peer authentication helpers: process/token/SID decoding and same-user identity checks. |

Owning module: [windows.rs](../windows.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
