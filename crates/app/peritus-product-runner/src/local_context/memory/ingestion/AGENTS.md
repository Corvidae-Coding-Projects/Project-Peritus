# crates/app/peritus-product-runner/src/local_context/memory/ingestion — navigation

## Responsibility

Implementation pieces for `ingestion.rs`: Persist-before-observe source ingestion and exact invocation input pinning.

## Files in this folder

| File | Role |
| --- | --- |
| [facts.rs](facts.rs) | Mechanical failed-approach records, derived only from completed host tool observations. |
| [pending.rs](pending.rs) | Deterministic host protocol projection; recovered proposals never execute here. |

Owning module: [ingestion.rs](../ingestion.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
