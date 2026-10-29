# crates/state/peritus-leases/src/transition/validation — navigation

## Responsibility

Implementation pieces for `validation.rs`: Shared validation and checked-successor helpers for pure reducers.

## Files in this folder

| File | Role |
| --- | --- |
| [claim.rs](claim.rs) | Exact active-claim matching shared by lease reducers. |

Owning module: [validation.rs](../validation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
