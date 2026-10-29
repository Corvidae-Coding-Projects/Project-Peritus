# crates/foundation/peritus-release-policy/src/evaluator/findings — navigation

## Responsibility

Implementation pieces for `findings.rs`: Finding and waiver policy reduction.

## Files in this folder

| File | Role |
| --- | --- |
| [lookup.rs](lookup.rs) | Exact executable lookup predicates for finding waivers. |
| [reduction.rs](reduction.rs) | Reduces release-evaluation findings into the criterion assessment. |

Owning module: [findings.rs](../findings.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
