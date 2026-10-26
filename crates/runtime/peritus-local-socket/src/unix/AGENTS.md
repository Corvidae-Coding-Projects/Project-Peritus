# crates/runtime/peritus-local-socket/src/unix — navigation

## Responsibility

Implementation pieces for `unix.rs`: Owner-checked custody of the private runtime directory used by an oversized endpoint.

## Files in this folder

| File | Role |
| --- | --- |
| [directory.rs](directory.rs) | Fail-closed creation and identity-bound cleanup of private runtime directories. |

Owning module: [unix.rs](../unix.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
