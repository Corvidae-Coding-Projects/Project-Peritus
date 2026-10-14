# crates/app/peritus-product-runner/src/control/files — navigation

## Responsibility

Implementation pieces for `files.rs`: Durable exact file references; source descriptors are inert, never filesystem capabilities.

## Files in this folder

| File | Role |
| --- | --- |
| [ledger.rs](ledger.rs) | Bounded immutable version history and explicit future-request selection. |
| [source.rs](source.rs) | Inert canonical path/range descriptors. The accepting host owns all read authorization. |
| [version.rs](version.rs) | Exact immutable observation metadata, distinct from a claim that authorization occurred. |

Owning module: [files.rs](../files.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
