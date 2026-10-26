# crates/runtime/peritus-patch/src/transaction/recover — navigation

## Responsibility

Implementation pieces for `recover.rs`: Restart recovery and conservative rollback.

## Files in this folder

| File | Role |
| --- | --- |
| [storage.rs](storage.rs) | Reads recovery manifests and quarantines inconsistent transaction storage. |

Owning module: [recover.rs](../recover.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
