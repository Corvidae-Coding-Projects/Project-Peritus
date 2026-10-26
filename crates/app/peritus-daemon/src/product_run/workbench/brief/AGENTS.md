# crates/app/peritus-daemon/src/product_run/workbench/brief — navigation

## Responsibility

Implementation pieces for `brief.rs`: Authenticated, read-only task-brief projection from the authoritative input ledger.

## Files in this folder

| File | Role |
| --- | --- |
| [pages.rs](pages.rs) | Paged metadata never opens reply bodies; exact body reads verify the retained publication. |

Owning module: [brief.rs](../brief.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
