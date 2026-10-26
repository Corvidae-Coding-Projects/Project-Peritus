# crates/app/peritus-daemon/src/ipc/windows/native — navigation

## Responsibility

Win32 named-pipe peer authentication helpers: process/token/SID decoding and same-user identity checks.

## Files in this folder

| File | Role |
| --- | --- |
| [identity.rs](identity.rs) | Bounded Win32 token and SID decoding. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
