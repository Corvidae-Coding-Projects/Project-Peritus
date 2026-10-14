# crates/state/peritus-journal/src/integrity — navigation

## Responsibility

Implementation pieces for `integrity.rs`: Integrity scan and exact export observations.

## Files in this folder

| File | Role |
| --- | --- |
| [artifacts.rs](artifacts.rs) | Integrity checks and export rows for committed artifact dependencies. |
| [scan.rs](scan.rs) | Complete transaction-scoped integrity scan orchestration. |
| [validation.rs](validation.rs) | Referential and exact-byte validation for mutable journal catalogs. |

Owning module: [integrity.rs](../integrity.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
