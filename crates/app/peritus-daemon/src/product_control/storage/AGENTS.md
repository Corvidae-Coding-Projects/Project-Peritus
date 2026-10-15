# crates/app/peritus-daemon/src/product_control/storage — navigation

## Responsibility

Implementation pieces for `storage.rs`: Exact operation replay, expected-revision state publication and original-receipt recovery.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoints.rs](checkpoints.rs) | Checkpoint before-images and restore transaction evidence in the control journal. |
| [files.rs](files.rs) | Atomic file-version bytes, source metadata and exact preview/admission proof retention. |
| [guidance.rs](guidance.rs) | Atomic guidance sidecar publication and authenticated rebuildable inspection. |
| [images.rs](images.rs) | Immutable image bytes published in the same C0 transaction as their input reference. |
| [initialization.rs](initialization.rs) | Atomic initialization audit and retained C1 installed-manifest evidence. |
| [library.rs](library.rs) | Atomic fork publication and rebuildable conversation-catalog projection. |
| [permissions.rs](permissions.rs) | Atomic workspace restriction sidecar publication in the existing control journal. |
| [replay.rs](replay.rs) | Immutable replay and exact historical checkpoint reads. |
| [reservations.rs](reservations.rs) | Child identities reserved by durable prepared restores under the single C0 owner. |

Owning module: [storage.rs](../storage.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
