# crates/foundation/peritus-budget/src/transition/lifecycle — navigation

## Responsibility

Implementation pieces for `lifecycle.rs`: Account sealing, closure, and begin-shape validation.

## Files in this folder

| File | Role |
| --- | --- |
| [close.rs](close.rs) | Account closure and release of unused delegated capacity. |
| [retry.rs](retry.rs) | Retry-history and attempt-charge validation. |
| [seal.rs](seal.rs) | Account sealing transitions. |

Owning module: [lifecycle.rs](../lifecycle.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
