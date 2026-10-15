# crates/runtime/peritus-process/src/supervisor/owner — navigation

## Responsibility

Implementation pieces for `owner.rs`: Common post-spawn ownership, cleanup, and terminal-publication funnel.

## Files in this folder

| File | Role |
| --- | --- |
| [finalize.rs](finalize.rs) | Post-loop cleanup, output closure, and durable terminal publication. |

Owning module: [owner.rs](../owner.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
