# crates/app/peritus-cli/src/product_run — navigation

## Responsibility

Implementation pieces for `product_run.rs`: Scriptable inspection and control of daemon-owned product runs.

## Files in this folder

| File | Role |
| --- | --- |
| [presentation.rs](presentation.rs) | Human and JSON projections of authoritative product-run observations. |
| [trace.rs](trace.rs) | Local retained-trace discovery and record-by-record human/JSON inspection, with early extension filtering. |

Owning module: [product_run.rs](../product_run.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
