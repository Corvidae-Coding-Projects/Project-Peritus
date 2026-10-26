# crates/state/peritus-journal/src/authority — navigation

## Responsibility

Implementation pieces for `authority.rs`: Durable authority-clock and credential-registry values.

## Files in this folder

| File | Role |
| --- | --- |
| [registry_commit.rs](registry_commit.rs) | Canonical credential-registry history construction and commit. |

Owning module: [authority.rs](../authority.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
