# crates/app/peritus-app-protocol/src/workbench/launch — navigation

## Responsibility

Implementation pieces for `launch.rs`: Typed launch profiles, scoped capture receipts, and independently classified result evidence.

## Files in this folder

| File | Role |
| --- | --- |
| [capture.rs](capture.rs) | Explicit capture controls, receipts, and artifact feedback primitives. |
| [output.rs](output.rs) | Bounded live output, separate from the stable launch-evidence wire format. |
| [profile.rs](profile.rs) | Bounded inert preview launch profiles and exact source identities. |
| [result.rs](result.rs) | Bounded launch results with independently classified evidence. |

Owning module: [launch.rs](../launch.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
