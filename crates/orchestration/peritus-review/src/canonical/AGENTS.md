# crates/orchestration/peritus-review/src/canonical — navigation

## Responsibility

Implementation pieces for `canonical.rs`: Domain-canonical D2 hashing.

## Files in this folder

| File | Role |
| --- | --- |
| [encoder.rs](encoder.rs) | Minimal canonical byte accumulator used only by D2 semantic digests. |

Owning module: [canonical.rs](../canonical.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
