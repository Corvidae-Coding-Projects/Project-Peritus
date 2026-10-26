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

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
