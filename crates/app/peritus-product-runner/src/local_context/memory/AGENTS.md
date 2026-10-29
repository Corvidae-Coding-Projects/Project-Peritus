# crates/app/peritus-product-runner/src/local_context/memory — navigation

## Responsibility

Implementation pieces for `memory.rs`: Durable state owner shared by the loop port and its bounded memory-tool decorator.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoint.rs](checkpoint.rs) | Persist exact prepared view and reconstruction artifacts before atomic publication. |
| [decoded.rs](decoded.rs) | Bounded, scope/digest-keyed decoded projections of immutable owned message artifacts; cold loads retain artifact validation. |
| [dependencies.rs](dependencies.rs) | Originating request dependencies, conservative legacy invalidation and durable version-6 reader-floor publication. |
| [environment.rs](environment.rs) | Exact host workspace/conversation bindings, with separately observed file dependencies. |
| [ingestion.rs](ingestion.rs) | Persist-before-observe source ingestion and exact invocation input pinning. |
| [recovery.rs](recovery.rs) | Exact checkpoint reconstruction plus an ordered uncovered journal suffix. |
| [source_index.rs](source_index.rs) | Checked constant-time source location, suffix and tool projections, numeric work counters, and a private reference-scan rollback path. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [ingestion/](ingestion/AGENTS.md) | Implementation pieces for `ingestion.rs`: Persist-before-observe source ingestion and exact invocation input pinning. |

Owning module: [memory.rs](../memory.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
