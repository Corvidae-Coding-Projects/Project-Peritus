# crates/app/peritus-daemon/src/command — navigation

## Responsibility

Implementation pieces for `command.rs`: Durable application-command admission and exact retained response construction.

## Files in this folder

| File | Role |
| --- | --- |
| [facts.rs](facts.rs) | Domain-separated digests for retained application command result facts. |
| [service.rs](service.rs) | A3 command binding to the durable C0 application ledger. |

Owning module: [command.rs](../command.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
