# crates/app/peritus-product-runner/src/local_context/memory — navigation

## Responsibility

Implementation pieces for `memory.rs`: Durable state owner shared by the loop port and its bounded memory-tool decorator.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoint.rs](checkpoint.rs) | Persist exact prepared view and reconstruction artifacts before atomic publication. |
| [environment.rs](environment.rs) | Exact host workspace/conversation bindings, with separately observed file dependencies. |
| [ingestion.rs](ingestion.rs) | Persist-before-observe source ingestion and exact invocation input pinning. |
| [recovery.rs](recovery.rs) | Exact checkpoint reconstruction plus an ordered uncovered journal suffix. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [ingestion/](ingestion/AGENTS.md) | Implementation pieces for `ingestion.rs`: Persist-before-observe source ingestion and exact invocation input pinning. |

Owning module: [memory.rs](../memory.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
