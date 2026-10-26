# crates/app/peritus-daemon/src/product_run/workbench/folder_mutation/init — navigation

## Responsibility

Implementation pieces for `init.rs`: Exact confirmed initialization, C1 effect, and durable original-receipt recovery.

## Files in this folder

| File | Role |
| --- | --- |
| [artifacts.rs](artifacts.rs) | Authenticated artifact initialization with the existing patch authority and receipt owner. |

Owning module: [init.rs](../init.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
