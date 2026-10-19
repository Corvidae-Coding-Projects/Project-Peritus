# crates/app/peritus-tui/src — navigation

## Responsibility

Native terminal application state, daemon event handling, interaction routing, rendering, connection lifecycle, and shared text editing.

## Files in this folder

| File | Role |
| --- | --- |
| [action.rs](action.rs) | Inputs and effects for the deterministic UI reducer. |
| [client.rs](client.rs) | Local daemon connection, A3 negotiation, and independently owned frame directions. |
| [entry.rs](entry.rs) | Process entry and strict TUI argument parsing. |
| [error.rs](error.rs) | Error boundary for TUI startup, transport, and terminal ownership. |
| [file_import.rs](file_import.rs) | Explicit external text snapshots with exact range and source-digest observations. |
| [help.rs](help.rs) | Key-reference content and bounded scrolling shared by input and rendering. |
| [image_import.rs](image_import.rs) | Explicit local image reads. No shell expansion, clipboard polling, or provider operations. |
| [input.rs](input.rs) | Keyboard-to-terminal byte mapping and line-editor helpers. |
| [highlight.rs](highlight.rs) | Bundled syntax grammars and dark/light/plain source and diff presentation. |
| [local.rs](local.rs) | Typed local presentation operations and their generation-fenced results. |
| [preferences.rs](preferences.rs) | Persistent convenience shortcuts and syntax theme, separate from provider settings. |
| [transcript_archive.rs](transcript_archive.rs) | Private incremental public-activity cache with literal local search and captured-history boundaries. |
| [lib.rs](lib.rs) | Interactive, protocol-faithful terminal client for Peritus. |
| [main.rs](main.rs) | `peritus-tui` interactive client entry point. Entry names: `main`. |
| [model.rs](model.rs) | Deterministic application state and update reducer. |
| [modes.rs](modes.rs) | Point-of-use explanations of existing conversation authority. |
| [render.rs](render.rs) | Ratatui rendering for every G2 interaction view. |
| [runtime.rs](runtime.rs) | Orderly terminal ownership and asynchronous application runtime. |
| [sanitize.rs](sanitize.rs) | Streaming terminal-output sanitization. |
| [terminal.rs](terminal.rs) | Terminal attachment transcript and A3 ordering state. |
| [title.rs](title.rs) | Scoped ownership of the Windows console title shared with foreground children. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [input/](input/AGENTS.md) | Terminal byte mapping, composer layout/caret movement, selection, text editing, and output browsing helpers. |
| [model/](model/AGENTS.md) | UI state and interaction transitions for conversations, workbench views, product controls, review, reconnect, and modal editing. |
| [render/](render/AGENTS.md) | Implementation pieces for `render.rs`: Ratatui rendering for every G2 interaction view. |
| [runtime/](runtime/AGENTS.md) | Implementation pieces for `runtime.rs`: Orderly terminal ownership and asynchronous application runtime. |
| [terminal/](terminal/AGENTS.md) | Implementation pieces for `terminal.rs`: Terminal attachment transcript and A3 ordering state. |
| [title/](title/AGENTS.md) | Implementation pieces for `title.rs`: Scoped ownership of the Windows console title shared with foreground children. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
