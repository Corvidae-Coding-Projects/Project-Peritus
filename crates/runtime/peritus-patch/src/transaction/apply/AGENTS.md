# crates/runtime/peritus-patch/src/transaction/apply — navigation

## Responsibility

Implementation pieces for `apply.rs`: Atomic multi-file application using staged finals and durable backups.

## Files in this folder

| File | Role |
| --- | --- |
| [install.rs](install.rs) | Installs staged patch operations, backups and final paths with durable sync boundaries. |
| [validation.rs](validation.rs) | Checks volume/platform modes and planned preimage/postimage bindings. |

Owning module: [apply.rs](../apply.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
