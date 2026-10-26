# crates/runtime/peritus-sandbox-macos/src/session — navigation

## Responsibility

Implementation pieces for `session.rs`: Supervised native session lifecycle.

## Files in this folder

| File | Role |
| --- | --- |
| [adapter.rs](adapter.rs) | C2 native-session trait adapter and stable error translation. |
| [cleanup.rs](cleanup.rs) | Idempotent normal and pre-activation session teardown. |
| [lifecycle.rs](lifecycle.rs) | Activated-session lifecycle transitions and bound observations. |
| [observation_mapping.rs](observation_mapping.rs) | Protected-handle validation and native control observations. |

Owning module: [session.rs](../session.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
