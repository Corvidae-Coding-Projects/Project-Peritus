# crates/app/peritus-launcher/src/app — navigation

## Responsibility

Implementation pieces for `app.rs`: End-to-end interactive product launch composition.

## Files in this folder

| File | Role |
| --- | --- |
| [diagnostics.rs](diagnostics.rs) | Redacted launch-time observations. Opening `/doctor` does not rerun bootstrap or setup. |
| [navigation.rs](navigation.rs) | Switch presentation scope without restarting the daemon or changing workspace authority. |

Owning module: [app.rs](../app.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
