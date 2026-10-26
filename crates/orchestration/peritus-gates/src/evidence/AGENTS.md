# crates/orchestration/peritus-gates/src/evidence — navigation

## Responsibility

Implementation pieces for `evidence.rs`: Exact-revision gate evidence publication contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [publication.rs](publication.rs) | Canonical request-specific evidence manifest. |
| [receipt.rs](receipt.rs) | Strict receipt construction and replay decoding. |

Owning module: [evidence.rs](../evidence.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
