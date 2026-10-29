# crates/runtime/peritus-process/src/supervisor — navigation

## Responsibility

Implementation pieces for `supervisor.rs`: Owned supervisor thread for process, I/O, cancellation, and terminal publication.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Retained spool publication into the content-addressed artifact store. |
| [finalization.rs](finalization.rs) | Deterministic terminal classification and resource observations. |
| [io.rs](io.rs) | Bounded process input, output-reader tasks, and spool accounting. |
| [owner.rs](owner.rs) | Common post-spawn ownership, cleanup, and terminal-publication funnel. |
| [ownership.rs](ownership.rs) | Bounded complete process-tree cleanup before terminal publication. |
| [resource.rs](resource.rs) | Local resource support admission, sampling, and ceiling enforcement. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [owner/](owner/AGENTS.md) | Implementation pieces for `owner.rs`: Common post-spawn ownership, cleanup, and terminal-publication funnel. |
| [resource/](resource/AGENTS.md) | Implementation pieces for `resource.rs`: Local resource support admission, sampling, and ceiling enforcement. |

Owning module: [supervisor.rs](../supervisor.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
