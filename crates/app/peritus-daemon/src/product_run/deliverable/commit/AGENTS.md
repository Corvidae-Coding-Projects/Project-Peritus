# crates/app/peritus-daemon/src/product_run/deliverable/commit — navigation

## Responsibility

Implementation pieces for `commit.rs`: Commit each selected file in the repository that owns it.

## Files in this folder

| File | Role |
| --- | --- |
| [attempt.rs](attempt.rs) | Durable source identity for retrying a partially completed nested commit. |
| [handoff.rs](handoff.rs) | Bind a successful explicit commit while preserving source-bound qualification evidence. |

Owning module: [commit.rs](../commit.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
