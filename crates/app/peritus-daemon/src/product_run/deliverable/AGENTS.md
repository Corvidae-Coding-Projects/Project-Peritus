# crates/app/peritus-daemon/src/product_run/deliverable — navigation

## Responsibility

Implementation pieces for `deliverable.rs`: Explicit accept, commit, export, and discard operations for completed deliverables.

## Files in this folder

| File | Role |
| --- | --- |
| [commit.rs](commit.rs) | Commit each selected file in the repository that owns it. |
| [discard.rs](discard.rs) | Durable acknowledgement of completed source restoration, independent of run projection saves. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [commit/](commit/AGENTS.md) | Implementation pieces for `commit.rs`: Commit each selected file in the repository that owns it. |
| [discard/](discard/AGENTS.md) | Implementation pieces for `discard.rs`: Durable acknowledgement of completed source restoration, independent of run projection saves. |

Owning module: [deliverable.rs](../deliverable.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
