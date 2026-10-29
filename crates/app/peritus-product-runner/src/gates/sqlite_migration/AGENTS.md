# crates/app/peritus-product-runner/src/gates/sqlite_migration — navigation

## Responsibility

Implementation pieces for `sqlite_migration.rs`: Explicit-plan `SQLite` migration checks on isolated copies of the declared initial database.

## Files in this folder

| File | Role |
| --- | --- |
| [assertion.rs](assertion.rs) | Parsing and execution of explicitly selected `SQLite` scalar postchecks. |

Owning module: [sqlite_migration.rs](../sqlite_migration.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
