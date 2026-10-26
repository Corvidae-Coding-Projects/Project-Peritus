# crates/state/peritus-leases/src/model/concrete — navigation

## Responsibility

Implementation pieces for `concrete.rs`: Refinement predicates over the executable lease representation.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Executable lease/policy intersection refinement predicates. |
| [fence_commands.rs](fence_commands.rs) | Command-specific exact accepted fencing contracts. |
| [fencing.rs](fencing.rs) | Executable fencing and reconciliation refinement predicates. |
| [identity.rs](identity.rs) | Exact nominal identity views used by executable validation refinement. |
| [preservation.rs](preservation.rs) | Exact aggregate preservation for consuming reducer rejection paths. |
| [rejections.rs](rejections.rs) | Exact total decision relations for every executable lease reducer. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [rejections/](rejections/AGENTS.md) | Implementation pieces for `rejections.rs`: Exact total decision relations for every executable lease reducer. |

Owning module: [concrete.rs](../concrete.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
