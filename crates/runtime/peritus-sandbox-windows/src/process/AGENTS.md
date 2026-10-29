# crates/runtime/peritus-sandbox-windows/src/process — navigation

## Responsibility

Implementation pieces for `process.rs`: Restricted token, `AppContainer`, Job Object, terminal, and handle policy.

## Files in this folder

| File | Role |
| --- | --- |
| [profile.rs](profile.rs) | Checked `AppContainer` profile construction and native identity derivation. |

Owning module: [process.rs](../process.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
