# crates/orchestration/peritus-run-knowledge/src/model — navigation

## Responsibility

Implementation pieces for `model.rs`: Input-defined logical model for run-knowledge freshness and reuse.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | Input-defined source collection admission and exact failure precedence. |
| [closure.rs](closure.rs) | Transitive dependency invalidation over exact successful plan entries. |
| [delta.rs](delta.rs) | Input-defined delta-packet admission, classification, and accounting. |
| [material.rs](material.rs) | Exact material equality used by production delta-packet planning. |
| [topology.rs](topology.rs) | Canonical snapshot topology and dependency-closure predicates. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [admission/](admission/AGENTS.md) | Implementation pieces for `admission.rs`: Input-defined source collection admission and exact failure precedence. |

Owning module: [model.rs](../model.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
