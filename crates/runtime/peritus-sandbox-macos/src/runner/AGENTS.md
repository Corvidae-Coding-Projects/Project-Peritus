# crates/runtime/peritus-sandbox-macos/src/runner — navigation

## Responsibility

Implementation pieces for `runner.rs`: Reserved helper exits and target handoff.

## Files in this folder

| File | Role |
| --- | --- |
| [materialized.rs](materialized.rs) | Rollback ownership for target secret files created before native activation completes. |
| [native.rs](native.rs) | Inventoried macOS Seatbelt, rlimit, and descriptor-hygiene boundary. |

Owning module: [runner.rs](../runner.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
