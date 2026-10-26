# crates/runtime/peritus-process/src/native — navigation

## Responsibility

Implementation pieces for `native.rs`: Process-owned extension point for authorized native sandbox sessions.

## Files in this folder

| File | Role |
| --- | --- |
| [observation.rs](observation.rs) | Validation of ordered, exact native lifecycle observations. |
| [probe.rs](probe.rs) | Production operating-system probe for durable process recovery. |
| [protected_handle.rs](protected_handle.rs) | Process-owned anonymous handles for native proxy and secret delivery. |
| [protocol.rs](protocol.rs) | Digest-bound native helper protocol records. |
| [pty.rs](pty.rs) | Target-side PTY attachment kept separate from helper protocol pipes. |
| [windows_channel.rs](windows_channel.rs) | Protected Windows helper status and terminal-control channels. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [probe/](probe/AGENTS.md) | Implementation pieces for `probe.rs`: Production operating-system probe for durable process recovery. |

Owning module: [native.rs](../native.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
