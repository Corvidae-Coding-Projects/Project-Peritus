# crates/app/peritus-daemon/src/outbox/qualification/snapshot — navigation

## Responsibility

Implementation pieces for `snapshot.rs`: Real Git snapshot recovery on both sides of retained-reference publication.

## Files in this folder

| File | Role |
| --- | --- |
| [corruption.rs](corruption.rs) | Controlled retained-reference divergence and production quarantine qualification. |
| [disk.rs](disk.rs) | Snapshot publication rollback when its durable manifest exceeds storage quota. |
| [git_command.rs](git_command.rs) | Fixed, bounded Git subprocess used only to seed and independently inspect qualification state. |

Owning module: [snapshot.rs](../snapshot.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
