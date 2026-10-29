# crates/orchestration/peritus-review/src/wire/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Canonical family-55 schema-v1 complete review-state checkpoint.

## Files in this folder

| File | Role |
| --- | --- |
| [disposition.rs](disposition.rs) | Canonical finding-disposition, fixer-response, evidence, and waiver wire encoding. |
| [finding.rs](finding.rs) | Canonical structured-finding wire encoding. |
| [summary.rs](summary.rs) | Canonical quorum, oscillation, and terminal-summary wire encoding. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
