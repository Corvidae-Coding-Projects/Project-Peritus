# crates/app/peritus-daemon/src/bin — navigation

## Responsibility

Executable entrypoints for `peritus-daemon`; follow each binary into its package implementation.

## Files in this folder

| File | Role |
| --- | --- |
| [peritusd.rs](peritusd.rs) | Peritus production daemon executable. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
