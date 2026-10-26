# crates/app/peritus-launcher/src/update — navigation

## Responsibility

Implementation pieces for `update.rs`: Cached public release discovery and native self-update composition.

## Files in this folder

| File | Role |
| --- | --- |
| [download.rs](download.rs) | Resumable checksum-bound release download and cancellable native extraction. |
| [install.rs](install.rs) | Transactional native installer invocation and installed-version verification. |
| [process.rs](process.rs) | Cancellable native update helpers without an execution deadline. |
| [release.rs](release.rs) | Exact release identity and current GitHub release discovery. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [install/](install/AGENTS.md) | Implementation pieces for `install.rs`: Transactional native installer invocation and installed-version verification. |
| [process/](process/AGENTS.md) | Implementation pieces for `process.rs`: Cancellable native update helpers without an execution deadline. |

Owning module: [update.rs](../update.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
