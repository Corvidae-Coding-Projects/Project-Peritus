# crates/app/peritus-cli/src — navigation

## Responsibility

Scriptable daemon client plus interactive launch dispatch: parse user arguments, route typed requests, render responses, and manage terminal/artifact/event streams.

## Files in this folder

| File | Role |
| --- | --- |
| [web.rs](web.rs) | Native web CLI operations and search setup/diagnostics. |
| [args.rs](args.rs) | Parses command families, global options, open/update/shutdown arguments, and exact input values. |
| [artifact.rs](artifact.rs) | Resumable artifact upload/download/cancellation and digest handling through the daemon client. |
| [client.rs](client.rs) | CLI error adaptation for the shared application client. |
| [completion.rs](completion.rs) | Generates supported-shell command completions. |
| [error.rs](error.rs) | CLI diagnostics and stable process exit categories. |
| [events.rs](events.rs) | Event subscription/delivery, acknowledgement, cancellation, and rendering. |
| [id.rs](id.rs) | Validates and renders canonical IDs/digests; generates request identifiers. |
| [improvements.rs](improvements.rs) | Scriptable candidate collection and explicit evaluation over authenticated daemon IPC. |
| [modules.rs](modules.rs) | Local module installation, listing, inspection and enablement; no code execution. |
| [lib.rs](lib.rs) | Scriptable command-line client for the local Peritus application protocol. |
| [main.rs](main.rs) | Process entry point for the `peritus` command-line client. Entry names: `main`. |
| [operation.rs](operation.rs) | Status, shutdown, command submission, and correlated daemon response handling. |
| [output.rs](output.rs) | Human/JSON output and terminal control-sequence sanitization. |
| [product_run.rs](product_run.rs) | Scriptable inspection and control of daemon-owned product runs. |
| [prompt.rs](prompt.rs) | Answers/cancels typed daemon prompts with session/input binding. |
| [runner.rs](runner.rs) | Process entry and command dispatch. |
| [terminal.rs](terminal.rs) | Owned terminal attachment, byte input, resize, detach, and cancellation requests. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [args/](args/AGENTS.md) | Implementation pieces for `args.rs`: Parses command families, global options, open/update/shutdown arguments, and exact input values. |
| [product_run/](product_run/AGENTS.md) | Implementation pieces for `product_run.rs`: Scriptable inspection and control of daemon-owned product runs. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
