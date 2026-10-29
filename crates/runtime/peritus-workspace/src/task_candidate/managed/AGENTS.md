# crates/runtime/peritus-workspace/src/task_candidate/managed — navigation

## Responsibility

Implementation pieces for `managed.rs`: Task-owned preimages, independent of HEAD and the user's index.

## Files in this folder

| File | Role |
| --- | --- |
| [capture.rs](capture.rs) | Raw-byte Git trees built with a private index; never runs clean/smudge filters. |
| [export.rs](export.rs) | Exact source patches across existing and task-created repositories. |
| [git_path.rs](git_path.rs) | Keep native filesystem paths distinct from Git tree names and local command operands. |
| [head.rs](head.rs) | Restore nested HEAD without rewriting or deleting any existing branch. |
| [index.rs](index.rs) | Preserve index stages, including unresolved merges, without touching the real index. |
| [paths.rs](paths.rs) | Validate restore paths and remove only task-owned structural replacements. |
| [recovery.rs](recovery.rs) | Recoverable removal of repositories that did not exist at task admission. |
| [restore.rs](restore.rs) | Restore task preimages without changing unrelated staged or unstaged files. |
| [retention.rs](retention.rs) | Independently retained nested Git history and metadata in the enclosing workspace. |
| [source_digest.rs](source_digest.rs) | Content identity for retrying a commit that only changed Git history; supports deriving this identity from the same full capture as the repository fingerprint. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [capture/](capture/AGENTS.md) | Implementation pieces for `capture.rs`: Raw-byte Git trees built with a private index; never runs clean/smudge filters. |
| [export/](export/AGENTS.md) | Implementation pieces for `export.rs`: Exact source patches across existing and task-created repositories. |
| [head/](head/AGENTS.md) | Implementation pieces for `head.rs`: Restore nested HEAD without rewriting or deleting any existing branch. |
| [restore/](restore/AGENTS.md) | Implementation pieces for `restore.rs`: Restore task preimages without changing unrelated staged or unstaged files. |
| [retention/](retention/AGENTS.md) | Implementation pieces for `retention.rs`: Independently retained nested Git history and metadata in the enclosing workspace. |
| [transaction/](transaction/AGENTS.md) | Durable explicit discard recovery; inspection never publishes restore effects. |

Owning module: [managed.rs](../managed.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
