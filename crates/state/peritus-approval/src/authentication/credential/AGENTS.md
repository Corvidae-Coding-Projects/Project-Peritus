# crates/state/peritus-approval/src/authentication/credential — navigation

## Responsibility

Implementation pieces for `credential.rs`: Canonical human credential and supplied registry snapshot values.

## Files in this folder

| File | Role |
| --- | --- |
| [projection.rs](projection.rs) | Closed credential and registry projections for public reducer contracts. |
| [specification.rs](specification.rs) | Exact credential-registry projections used by reducer contracts. |

Owning module: [credential.rs](../credential.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
