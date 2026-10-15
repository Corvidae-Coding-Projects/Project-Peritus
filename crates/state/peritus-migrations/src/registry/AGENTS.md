# crates/state/peritus-migrations/src/registry — navigation

## Responsibility

Implementation pieces for `registry.rs`: Initial release schema marker and future forward-migration registry.

## Files in this folder

| File | Role |
| --- | --- |
| [v1.rs](v1.rs) | Bootstrap migration source and reviewed digest. |
| [v2.rs](v2.rs) | Independent logical artifact identities sharing content-addressed bytes. |
| [v3.rs](v3.rs) | Preserve exact history nodes while removing synthetic logical state capacities. |
| [v4.rs](v4.rs) | Preserve state bytes and receipts while moving large values out of the lookup key tree. |
| [validation.rs](validation.rs) | Validation helpers for reviewed migration descriptors. |

Owning module: [registry.rs](../registry.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
