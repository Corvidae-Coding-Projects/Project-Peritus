# crates/app/peritus-daemon/src/product_control — navigation

## Responsibility

Implementation pieces for `product_control.rs`: Daemon-owned authoritative C0 product-control journal. Legacy run JSON is not a writer here.

## Files in this folder

| File | Role |
| --- | --- |
| [compaction.rs](compaction.rs) | Deterministic local prompt-view proposals over source-bound immutable public replies. |
| [context.rs](context.rs) | Authenticated read-only context inspection. No recovery, provider, workspace read, or mutation. |
| [error.rs](error.rs) | Typed storage failures with preserved sources and redaction-safe public descriptions. |
| [goal.rs](goal.rs) | Host-only durable goal admission, accounting, progress, and settlement operations. |
| [guidance.rs](guidance.rs) | C0 planning and replay for explicit project-local guidance and content-free tombstones. |
| [init.rs](init.rs) | Local project discovery and exact reviewed initialization patch preparation. |
| [inputs.rs](inputs.rs) | Host-only request binding; clients cannot manufacture an incorporation grant. |
| [replies.rs](replies.rs) | Exact public reply delivery and source-bound reconstruction for later user turns. |
| [storage.rs](storage.rs) | Exact operation replay, expected-revision state publication and original-receipt recovery. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [goal/](goal/AGENTS.md) | Implementation pieces for `goal.rs`: Host-only durable goal admission, accounting, progress, and settlement operations. |
| [guidance/](guidance/AGENTS.md) | Implementation pieces for `guidance.rs`: C0 planning and replay for explicit project-local guidance and content-free tombstones. |
| [init/](init/AGENTS.md) | Implementation pieces for `init.rs`: Local project discovery and exact reviewed initialization patch preparation. |
| [inputs/](inputs/AGENTS.md) | Implementation pieces for `inputs.rs`: Host-only request binding; clients cannot manufacture an incorporation grant. |
| [storage/](storage/AGENTS.md) | Implementation pieces for `storage.rs`: Exact operation replay, expected-revision state publication and original-receipt recovery. |

Owning module: [product_control.rs](../product_control.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
