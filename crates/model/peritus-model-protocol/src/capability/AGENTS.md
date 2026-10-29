# crates/model/peritus-model-protocol/src/capability — navigation

## Responsibility

Implementation pieces for `capability.rs`: Explicit provider capability profiles and pure negotiation.

## Files in this folder

| File | Role |
| --- | --- |
| [feature.rs](feature.rs) | Capability identities and closed support matrices. |
| [negotiation.rs](negotiation.rs) | Pure checked capability and limit negotiation. |
| [profile.rs](profile.rs) | Immutable provider profiles, model limits, and lifecycle semantics. |

Owning module: [capability.rs](../capability.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
