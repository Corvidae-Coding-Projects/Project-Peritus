# Long-history daemon/TUI replay

## Folder guide

Real-interface replay resources for daemon history; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [probe.rs](probe.rs) | Local implementation for docs / testing / reproducers / daemon-history. Entry names: `Error`, `connect`, `request`, `main`. |
| [replay.py](replay.py) | Real terminal, daemon, and persisted history replay; no mocked application. |
| [screen.rs](screen.rs) | Local implementation for docs / testing / reproducers / daemon-history. Entry names: `main`. |

Use an isolated copy of a real Peritus state directory and the actual current binaries. The fixture must contain `bin/peritus`, `bin/peritusd`, `bin/peritus-tui`, and separate `config`, `state`, and `cache` directories. Mark only a disposable fixture with `real-test-fixture.json` containing `{"owned_disposable_fixture": true}`. Never mark the live installation.

Copy product state, workspace registrations, public approval registry, workbench run records and continuations. Use SQLite's backup API for a coherent copy of `workbench-v1/control.sqlite3`. Copy effect ledgers if testing run-list projection, but never copy native process ownership or the core daemon database: the fixture must not own the user's real child processes. Recreate generated daemon configurations for the new XDG paths through `peritus open`; copied generated configurations contain their original absolute paths. No request in this replay starts, retries, or resumes inference. Workspace contents are only viewed.

Build `probe.rs` as a standalone consumer of the checkout's public `peritus-app-client`, `peritus-app-protocol`, and `peritus-types` crates, with Tokio 1.53.1 (`macros`, `rt-multi-thread`, `time`). Build `screen.rs` separately with vt100 0.16.2. Use two Cargo jobs and a shared target directory. The screen consumer decodes the real terminal output; it does not substitute Peritus rendering.

```sh
PROBE ENDPOINT RUN_ID 5
PROBE ENDPOINT RUN_ID 5 exercise
python3 docs/testing/reproducers/daemon-history/replay.py \
  --root OWNED_FIXTURE --workspace REGISTERED_WORKSPACE --run RUN_ID \
  --screen-parser SCREEN_BINARY
```

The first probe measures real authenticated binding requests without mutating application state. `exercise` operates only the copied conversation: twelve alternating pin changes, original-operation replay, stale-revision rejection, fresh clients, and four distinct Unicode/multiline queued inputs. Each change must advance exactly once, stale changes must preserve state, pinning must return to its original value, and accepted input must appear exactly in the public conversation projection. Queue acceptance does not start inference.

The TUI driver reopens retained history, types an unsent draft, reconnects, observes 60 seconds of ordinary refresh/heartbeats, pauses only its owned CLI for 17 seconds, resumes, gracefully shuts down only the fixture daemon, waits beyond the notice lifetime, and reconnects through the real launcher. It checks the final terminal screen, draft preservation, continued connection, durable restart and the persistent disconnect reason. It records CPU sampling, process identities, terminal captures and screens. It kills only its own TUI on failure; the fixture daemon can be stopped with `peritus --endpoint ENDPOINT shutdown --wait`.

For cache invalidation, warm the real daemon using the probe, change one copied journal state row through a second SQLite connection, and call the probe again. Damaging root bytes without updating their digest must return an integrity error. Restore the exact original bytes in `finally`; a fresh request must replay successfully and subsequent reads must become fast again. This is fault injection into a real disposable database, not a mocked storage implementation.

Evidence and constraints are in [the campaign report](../../daemon-disconnects-2026-10-12.md). Large private conversation fixtures and raw personal text are retained locally, not committed.
