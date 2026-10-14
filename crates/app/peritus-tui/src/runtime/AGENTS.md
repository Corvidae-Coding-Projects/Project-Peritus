# crates/app/peritus-tui/src/runtime — navigation

## Responsibility

Implementation pieces for `runtime.rs`: Orderly terminal ownership and asynchronous application runtime.

## Files in this folder

| File | Role |
| --- | --- |
| [candidate.rs](candidate.rs) | Foreground execution of a selected candidate's documented run command. |
| [clipboard.rs](clipboard.rs) | One owned clipboard write at a time, using desktop helpers or terminal OSC 52. |
| [connection.rs](connection.rs) | An owned connection attempt is polled beside input, never in front of the UI loop. |
| [files.rs](files.rs) | One owned bounded external-text read at a time; the event loop stays interactive. |
| [images.rs](images.rs) | One owned bounded file read at a time; the event loop stays interactive. |
| [interrupts.rs](interrupts.rs) | Keep process interrupts owned by the UI while a foreground child shares its terminal. |
| [product.rs](product.rs) | Launcher-resolved product facts displayed by the interactive client. |
| [state.rs](state.rs) | Process-local presentation retention across a launcher-owned daemon recovery. |
| [terminal.rs](terminal.rs) | Exclusive terminal ownership and the blocking terminal-input worker. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [candidate/](candidate/AGENTS.md) | Implementation pieces for `candidate.rs`: Foreground execution of a selected candidate's documented run command. |
| [connection/](connection/AGENTS.md) | Implementation pieces for `connection.rs`: An owned connection attempt is polled beside input, never in front of the UI loop. |

Owning module: [runtime.rs](../runtime.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
