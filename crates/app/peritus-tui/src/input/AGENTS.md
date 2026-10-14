# crates/app/peritus-tui/src/input — navigation

## Responsibility

Terminal byte mapping, composer layout/caret movement, selection, text editing, and output browsing helpers.

## Files in this folder

| File | Role |
| --- | --- |
| [composer.rs](composer.rs) | One character-cell layout for composer rendering, cursor placement, and mouse hit testing. |
| [output.rs](output.rs) | Selection of a frozen visible transcript, preserving UTF-8 and hard line breaks. |
| [selection.rs](selection.rs) | UTF-8 selection and replacement semantics for the message composer. |
| [terminal.rs](terminal.rs) | Xterm input encoding, preserving modifiers instead of sending an unmodified key. |

Owning module: [input.rs](../input.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
