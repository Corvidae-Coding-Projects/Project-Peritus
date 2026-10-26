# crates/app/peritus-product-runner/src/control/record/transition — navigation

## Responsibility

Implementation pieces for `transition.rs`: Pure expected-revision successor planning and receipt binding.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoint.rs](checkpoint.rs) | Checkpoint and restore aggregate transitions. |
| [goal.rs](goal.rs) | Goal lifecycle, accounting, fork, and evidence transitions. |
| [mutations.rs](mutations.rs) | Input, goal, attachment, review, and fork aggregate transitions. |

Owning module: [transition.rs](../transition.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
