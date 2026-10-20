# peritus-cli

## Folder guide

Scriptable command-line client for the Peritus daemon.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-cli`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Scriptable daemon client plus interactive launch dispatch: parse user arguments, route typed requests, render responses, and manage terminal/artifact/event streams. |

G1 owns the scriptable `peritus` command-line client. The crate connects only to the protected
local G0 endpoint and expresses every operation through the A3 application protocol. It provides
strict argument parsing, stable JSON and human output, stable exit categories, resumable event and
artifact streams, prompt responses, terminal control, and shell completions.

The `runs` command family lists and inspects durable coding runs and can continue, execute, export,
accept, commit, discard, retry, or cancel one exact run. Human and JSON output include settlement
cause, candidate digest, qualification evidence, changed paths, successful checks, and run
instructions. Accepting or committing an unqualified candidate requires repeating the command with
the exact digest printed by the client.

`runs trace --run <ID> [--extensions]` reads one run's durable private provider trace without a
daemon session. Omit `--run` to list retained traces newest first; the human and JSON results
include each run identifier for the trace command.
Trace inspection processes one selected frame at a time. `--extensions` skips other frame kinds
before reading their payloads. Human and JSON output are rendered separately into an automatically
removed file in the normal Peritus cache, then copied to stdout, preserving all-or-error output with memory tied to
one selected frame rather than the whole trace. Temporary storage is required for the selected
output; the system temporary directory is not used because it may be RAM-backed. I/O failures
are reported without a partial success document.

`peritus shutdown` discovers the current user's running daemon and waits for orderly shutdown and
release of its state lock. It needs no endpoint, provider login, setup, or sibling executable, and
reports success when the daemon is already stopped. It does not start a daemon or change settings.
Automation can still select an explicit address with `--endpoint`; its existing `--wait` option
retains the authenticated connection while G0 sends
six bounded progress observations and one correlated clean or unclean completion. It no longer
mistakes orderly endpoint withdrawal for an I/O failure.

The client never grants authority or infers durable success. G0 authenticates the peer and session,
checks current authority, commits commands, and reports typed outcomes. This crate depends only on
the application protocol, canonical codec, foundational types, and its local async transport.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
