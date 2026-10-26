# crates/app/peritus-app-protocol/src/schema/fields/types/protocol — navigation

## Responsibility

Implementation pieces for `protocol.rs`: Negotiation and shared protocol field metadata groups.

## Files in this folder

| File | Role |
| --- | --- |
| [handshake.rs](handshake.rs) | Negotiation hello and outer-envelope field metadata. |
| [version_limits.rs](version_limits.rs) | Version, context, resource-limit, and revision field metadata. |

Owning module: [protocol.rs](../protocol.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
