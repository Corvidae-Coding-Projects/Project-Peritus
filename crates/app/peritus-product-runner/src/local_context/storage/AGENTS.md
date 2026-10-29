# crates/app/peritus-product-runner/src/local_context/storage — navigation

## Responsibility

Implementation pieces for `storage.rs`: Single-owner C0 journal/artifact storage and atomic checkpoint-root publication.

## Files in this folder

| File | Role |
| --- | --- |
| [work.rs](work.rs) | Non-authoritative content-free counters for local store operations and checkpoint work. |
| [append.rs](append.rs) | One C0 transaction binds exact events, finalized artifact roots, and checkpoint CAS. |
| [artifact_wire.rs](artifact_wire.rs) | Explicit digest-array wire form without generated custom-field serializer hooks. |
| [identity.rs](identity.rs) | Stable lineage-derived C0 identities with no provider or invocation component. |
| [inspection.rs](inspection.rs) | Read-only inspection of one exact published checkpoint while its writer may still be active. |
| [location.rs](location.rs) | Validate the existing ancestor before creating any run-memory directory. |

Owning module: [storage.rs](../storage.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
