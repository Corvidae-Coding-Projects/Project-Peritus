# crates/app/peritus-daemon/src/product_control/guidance — navigation

## Responsibility

Implementation pieces for `guidance.rs`: C0 planning and replay for explicit project-local guidance and content-free tombstones.

## Files in this folder

| File | Role |
| --- | --- |
| [stored.rs](stored.rs) | Canonical guidance storage DTOs; field order and serde representation are stable. |

Owning module: [guidance.rs](../guidance.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
