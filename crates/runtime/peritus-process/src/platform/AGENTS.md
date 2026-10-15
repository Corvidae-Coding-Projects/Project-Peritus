# crates/runtime/peritus-process/src/platform — navigation

## Responsibility

Implementation pieces for `platform.rs`: Narrow operating-system launch adapters.

## Files in this folder

| File | Role |
| --- | --- |
| [inheritance.rs](inheritance.rs) | Exact protected-handle inheritance for the direct native helper child. |
| [linux_tree.rs](linux_tree.rs) | Linux session ownership across ordinary job-control process groups. |
| [ownership.rs](ownership.rs) | Durable process-tree identity observations. |
| [pipe.rs](pipe.rs) | Direct structured pipe process launch with process-tree containment. |
| [pty.rs](pty.rs) | Direct structured PTY process launch and session ownership. |
| [resource.rs](resource.rs) | Platform resource observations used by supervisor enforcement. |
| [self_memory.rs](self_memory.rs) | Read-only Windows host-process accounting; does not open or control other processes. |
| [watchdog.rs](watchdog.rs) | Independent Linux owner-death watchdog for exact process groups. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [pipe/](pipe/AGENTS.md) | Implementation pieces for `pipe.rs`: Direct structured pipe process launch with process-tree containment. |
| [pty/](pty/AGENTS.md) | Implementation pieces for `pty.rs`: Direct structured PTY process launch and session ownership. |
| [resource/](resource/AGENTS.md) | Implementation pieces for `resource.rs`: Platform resource observations used by supervisor enforcement. |

Owning module: [platform.rs](../platform.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
