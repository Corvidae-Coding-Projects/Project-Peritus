# crates/foundation/peritus-quality-policy/src/evidence — navigation

## Responsibility

Implementation pieces for `evidence.rs`: Canonical evidence observations and checked evidence aggregate.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Canonical approval subjects and waiver-request consistency. |
| [ordering.rs](ordering.rs) | Exact gate and artifact collection ordering, preserving first-error precedence. |
| [reviews.rs](reviews.rs) | Review collection ordering and cross-review finding uniqueness. |

Owning module: [evidence.rs](../evidence.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
