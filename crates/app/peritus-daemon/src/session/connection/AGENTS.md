# crates/app/peritus-daemon/src/session/connection — navigation

## Responsibility

Implementation pieces for `connection.rs`: One bounded A3 connection owner with independent read-only provider discovery.

## Files in this folder

| File | Role |
| --- | --- |
| [select.rs](select.rs) | Cancellation-safe receive selection keeps provider reads independent of client traffic. |

Owning module: [connection.rs](../connection.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
