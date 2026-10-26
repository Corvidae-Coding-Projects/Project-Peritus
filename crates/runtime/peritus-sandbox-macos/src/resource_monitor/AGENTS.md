# crates/runtime/peritus-sandbox-macos/src/resource_monitor — navigation

## Responsibility

Implementation pieces for `resource_monitor.rs`: Bounded macOS process-group and workspace resource sampling.

## Files in this folder

| File | Role |
| --- | --- |
| [pids.rs](pids.rs) | Complete bounded libproc process-group counts for required resource samples. |

Owning module: [resource_monitor.rs](../resource_monitor.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
