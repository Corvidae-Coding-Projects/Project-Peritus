# crates/state/peritus-approval/src/authentication — navigation

## Responsibility

Implementation pieces for `authentication.rs`: Bounded credential snapshots and the sole strict Ed25519 verifier surface.

## Files in this folder

| File | Role |
| --- | --- |
| [credential.rs](credential.rs) | Canonical human credential and supplied registry snapshot values. |
| [observation.rs](observation.rs) | Opaque authenticated facts returned only by strict decision verification. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [credential/](credential/AGENTS.md) | Implementation pieces for `credential.rs`: Canonical human credential and supplied registry snapshot values. |

Owning module: [authentication.rs](../authentication.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
