# crates/runtime/peritus-process/src/supervisor/resource — navigation

## Responsibility

Implementation pieces for `resource.rs`: Local resource support admission, sampling, and ceiling enforcement.

## Files in this folder

| File | Role |
| --- | --- |
| [disk.rs](disk.rs) | Sample a changing workspace without treating ordinary file deletion as owner failure. |

Owning module: [resource.rs](../resource.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
