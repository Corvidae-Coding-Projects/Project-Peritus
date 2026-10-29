# crates/foundation/peritus-quality-policy/src/evaluator/authority — navigation

## Responsibility

Implementation pieces for `authority.rs`: Blocker, waiver, and final human-approval evaluation.

## Files in this folder

| File | Role |
| --- | --- |
| [blockers.rs](blockers.rs) | Exact blocker resolution over current reviews and the contract severity threshold. |
| [diagnostics.rs](diagnostics.rs) | Monotonic preservation of already emitted waiver diagnostics. |
| [lookup.rs](lookup.rs) | Exact current-observation lookup over constructor-proved unique identities. |
| [supplied.rs](supplied.rs) | Validation and diagnostic completeness for every supplied current waiver. |
| [waiver.rs](waiver.rs) | Policy authority and approval refinement for a selected finding and waiver. |

Owning module: [authority.rs](../authority.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
