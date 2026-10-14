# crates/app/peritus-tui/src/terminal — navigation

## Responsibility

Implementation pieces for `terminal.rs`: Terminal attachment transcript and A3 ordering state.

## Files in this folder

| File | Role |
| --- | --- |
| [line_input.rs](line_input.rs) | Bounded local editing for pipe-backed children that have no terminal line discipline. |
| [transcript.rs](transcript.rs) | Logical terminal history shared by attachments and complete preview records. |

Owning module: [terminal.rs](../terminal.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
