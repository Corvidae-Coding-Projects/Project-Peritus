# crates/app/peritus-product-runner/src/context_config — navigation

## Responsibility

Implementation pieces for `context_config.rs`: Explicit local-only context policy and optional bounded subprocess configuration.

## Files in this folder

| File | Role |
| --- | --- |
| [sandbox_wire.rs](sandbox_wire.rs) | Explicit serde bridge preserves strict platform-tagged operator configuration. |

Owning module: [context_config.rs](../context_config.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
