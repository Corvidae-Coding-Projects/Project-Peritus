# crates/foundation/peritus-policy/src/amendment — navigation

## Responsibility

Implementation pieces for `amendment.rs`: Explicit single-tier policy-amendment preview without activation authority.

## Files in this folder

| File | Role |
| --- | --- |
| [preview.rs](preview.rs) | Closed total amendment-preview result relation and executable refinement. |
| [proposal.rs](proposal.rs) | Exact checked amendment-proposal construction and projections. |

Owning module: [amendment.rs](../amendment.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
