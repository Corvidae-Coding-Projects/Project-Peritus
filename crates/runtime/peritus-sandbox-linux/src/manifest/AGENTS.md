# crates/runtime/peritus-sandbox-linux/src/manifest — navigation

## Responsibility

Implementation pieces for `manifest.rs`: Versioned bounded helper manifest.

## Files in this folder

| File | Role |
| --- | --- |
| [activation.rs](activation.rs) | Fixed helper activation record used by backend-local diagnostics. |
| [delivery.rs](delivery.rs) | Canonical secret-destination encoding without secret payload material. |
| [value.rs](value.rs) | Bounded literal values carried by the Linux helper manifest. |

Owning module: [manifest.rs](../manifest.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
