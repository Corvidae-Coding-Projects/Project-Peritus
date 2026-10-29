# crates/app/peritus-product-runner/src/gates — navigation

## Responsibility

Implementation pieces for `gates.rs`: Candidate-aware D1 gate execution adapter.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact_csv.rs](artifact_csv.rs) | Contract-selected, incremental structural validation for changed CSV artifacts. |
| [cancellation.rs](cancellation.rs) | Cooperative cancellation for streamed host-owned gate reads. |
| [coverage.rs](coverage.rs) | Missing check contracts are harness prerequisites, not candidate defects for a fixer. |
| [deliverable_inventory.rs](deliverable_inventory.rs) | Deterministic reconciliation of closed deliverable requests with final changed paths. |
| [execution_policy.rs](execution_policy.rs) | Authoritative task restrictions fence automatic command execution before any gate starts. |
| [explicit_paths.rs](explicit_paths.rs) | Deterministic reconciliation of literal task paths with the candidate workspace. |
| [json_structure.rs](json_structure.rs) | Deterministic syntax validation for changed JSON files. |
| [source_layout.rs](source_layout.rs) | Deterministic source-file readability verification for product candidates. |
| [sqlite_migration.rs](sqlite_migration.rs) | Explicit-plan `SQLite` migration checks on isolated copies of the declared initial database. |
| [yaml_structure.rs](yaml_structure.rs) | Deterministic syntax validation for changed YAML configuration files. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [artifact_csv/](artifact_csv/AGENTS.md) | Implementation pieces for `artifact_csv.rs`: Contract-selected, incremental structural validation for changed CSV artifacts. |
| [explicit_paths/](explicit_paths/AGENTS.md) | Implementation pieces for `explicit_paths.rs`: Deterministic reconciliation of literal task paths with the candidate workspace. |
| [sqlite_migration/](sqlite_migration/AGENTS.md) | Implementation pieces for `sqlite_migration.rs`: Explicit-plan `SQLite` migration checks on isolated copies of the declared initial database. |

Owning module: [gates.rs](../gates.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
