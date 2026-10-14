# crates/app/peritus-tui/src/runtime/candidate — navigation

## Responsibility

Implementation pieces for `candidate.rs`: Foreground execution of a selected candidate's documented run command.

## Files in this folder

| File | Role |
| --- | --- |
| [foreground.rs](foreground.rs) | Transfer terminal job control to the child and restore it before resuming the UI. |

Owning module: [candidate.rs](../candidate.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
