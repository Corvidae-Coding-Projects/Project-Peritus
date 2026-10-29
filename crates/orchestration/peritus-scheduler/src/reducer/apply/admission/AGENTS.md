# crates/orchestration/peritus-scheduler/src/reducer/apply/admission — navigation

## Responsibility

Implementation pieces for `admission.rs`: Production work admission with exact rejection and successor contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [checks.rs](checks.rs) | Exact bounded scans used by production work admission. |

Owning module: [admission.rs](../admission.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
