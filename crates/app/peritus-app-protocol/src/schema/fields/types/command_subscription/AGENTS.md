# crates/app/peritus-app-protocol/src/schema/fields/types/command_subscription — navigation

## Responsibility

Implementation pieces for `command_subscription.rs`: Command and subscription field metadata groups.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Command binding, result, and public error field metadata. |
| [control.rs](control.rs) | Subscription gap, acknowledgement, cancellation, and control field metadata. |
| [delivery.rs](delivery.rs) | Subscription request and delivery field metadata. |

Owning module: [command_subscription.rs](../command_subscription.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
