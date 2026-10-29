# crates/state/peritus-journal/src/record — navigation

## Responsibility

Implementation pieces for `record.rs`: Exact frame records and checked state-install inputs.

## Files in this folder

| File | Role |
| --- | --- |
| [committed.rs](committed.rs) | Digest-checked immutable event observations. |
| [state.rs](state.rs) | Journal-owned durable state values and compare-and-swap installs. |

Owning module: [record.rs](../record.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
