# crates/app/peritus-daemon/src/qualification — navigation

## Responsibility

Implementation pieces for `qualification.rs`: Production qualification seams shared by component-specific fault routes.

## Files in this folder

| File | Role |
| --- | --- |
| [blob_corruption.rs](blob_corruption.rs) | Controlled artifact corruption followed by durable startup containment. |
| [daemon_lifecycle.rs](daemon_lifecycle.rs) | Real staged-daemon kill checkpoints over durable E0/C0 lifecycle state. |
| [dependency.rs](dependency.rs) | Real provider, product-tool, and worker failure qualification over durable scheduler state. |
| [disk.rs](disk.rs) | Real artifact-finalization quota exhaustion and restart verification. |
| [disk_journal.rs](disk_journal.rs) | Deterministic authoritative-journal page exhaustion and rollback verification. |
| [evidence_corruption.rs](evidence_corruption.rs) | Controlled acceptance-evidence corruption and durable startup containment. |
| [journal.rs](journal.rs) | Shared authoritative-journal access for production qualification routes. |
| [journal_corruption.rs](journal_corruption.rs) | Controlled authoritative-journal corruption and fail-closed startup qualification. |
| [projection.rs](projection.rs) | Controlled corruption and real startup repair of a durable derived projection. |
| [promotion_evidence_corruption.rs](promotion_evidence_corruption.rs) | Controlled corruption of genuinely published F0 harness-activation evidence. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [dependency/](dependency/AGENTS.md) | Implementation pieces for `dependency.rs`: Real provider, product-tool, and worker failure qualification over durable scheduler state. |

Owning module: [qualification.rs](../qualification.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
