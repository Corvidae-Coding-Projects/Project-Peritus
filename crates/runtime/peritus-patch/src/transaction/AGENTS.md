# crates/runtime/peritus-patch/src/transaction — navigation

## Responsibility

Recoverable staged-final and backup transaction adapter.

## Files in this folder

| File | Role |
| --- | --- |
| [apply.rs](apply.rs) | Atomic multi-file application using staged finals and durable backups. |
| [filesystem.rs](filesystem.rs) | Checked filesystem observations shared by application and recovery. |
| [manifest.rs](manifest.rs) | Bounded versioned transaction manifest encoding. |
| [mod.rs](mod.rs) | Recoverable staged-final and backup transaction adapter. |
| [recover.rs](recover.rs) | Restart recovery and conservative rollback. |
| [recovery_observation.rs](recovery_observation.rs) | Read-only classification of restart-visible target and backup states. |
| [roots.rs](roots.rs) | Workspace and protected transaction root validation. |
| [storage.rs](storage.rs) | Staged-file and durable manifest storage inside one transaction directory. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [apply/](apply/AGENTS.md) | Implementation pieces for `apply.rs`: Atomic multi-file application using staged finals and durable backups. |
| [recover/](recover/AGENTS.md) | Implementation pieces for `recover.rs`: Restart recovery and conservative rollback. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
