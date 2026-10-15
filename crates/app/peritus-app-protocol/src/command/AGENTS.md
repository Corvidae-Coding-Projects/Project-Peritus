# crates/app/peritus-app-protocol/src/command — navigation

## Responsibility

Exact B3 command submission binding, final results, and bounded idempotency.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Lossless B3 envelope/command parsing and outer request binding. |
| [idempotency.rs](idempotency.rs) | Bounded pure final-result idempotency window. |
| [mod.rs](mod.rs) | Exact B3 command submission binding, final results, and bounded idempotency. |
| [result.rs](result.rs) | Final protocol-level command disposition and committed event ranges. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
