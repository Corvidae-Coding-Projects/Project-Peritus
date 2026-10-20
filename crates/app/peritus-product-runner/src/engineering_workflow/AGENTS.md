# crates/app/peritus-product-runner/src/engineering_workflow — navigation

## Responsibility

Host-owned role cues and conditional Rust activation. Skill documents and resources live only in [canonical skills](../../../../../skills/AGENTS.md) and are discovered at build time.

## Files in this folder

| File | Role |
| --- | --- |
| [rust_reliability.rs](rust_reliability.rs) | Host-observed Rust activation and drift reminders; resources come from the generated canonical bundle. |
| [vitamins.rs](vitamins.rs) | Small role-specific cues adapted from the user-provided prompt catalogue. |

Owning module: [engineering_workflow.rs](../engineering_workflow.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
