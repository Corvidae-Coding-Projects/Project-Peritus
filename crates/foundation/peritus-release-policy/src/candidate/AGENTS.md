# crates/foundation/peritus-release-policy/src/candidate — navigation

## Responsibility

Implementation pieces for `candidate.rs`: Exact release-candidate, platform, toolchain, profile, and schema identities.

## Files in this folder

| File | Role |
| --- | --- |
| [construction.rs](construction.rs) | Exact checked construction of complete release-candidate components. |
| [equality.rs](equality.rs) | Executable correspondence for complete release-candidate identity. |
| [git.rs](git.rs) | Checked Git commit identities. |
| [platform.rs](platform.rs) | Exact tier-one platform matrix identities. |
| [version.rs](version.rs) | Exact release-version identity. |

Owning module: [candidate.rs](../candidate.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
