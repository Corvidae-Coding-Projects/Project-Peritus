# crates/orchestration/peritus-review/src/finding — navigation

## Responsibility

Implementation pieces for `finding.rs`: Checked structured review submissions and complete finding history.

## Files in this folder

| File | Role |
| --- | --- |
| [location.rs](location.rs) | Checked repository-relative source locations. |
| [submission.rs](submission.rs) | Checked structured submission aggregate. |

Owning module: [finding.rs](../finding.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
