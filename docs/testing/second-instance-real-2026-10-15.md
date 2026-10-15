# Second-window daemon continuity — real testing, 2026-10-15

## Result

The installed launcher replaced the shared daemon when another window selected a new
folder. The corrected launcher and daemon passed nine isolated real-terminal journeys,
including overlapping requests to the actual Codex provider. Every final journey retained
the daemon PID and process start token, reconnected the first client, and left it usable
after closing the second client. Second-window readiness took 1.944–2.435 seconds with
the tested development binaries and disposable state.

This fixes the reproduced restart/disconnection mechanism. The original long-running
workspace's timeout was not deliberately reproduced: restarting its live work would be
disruptive. Recovery after an unnecessary restart is a plausible explanation for its
longer delay, rather than a separately measured timing result.

The initial isolated campaign preserved the installed binaries and live daemon. After
explicit user authorization, commit `e3f22be5f` was built in release mode, installed, and
the browser run was resumed. See the deployment observations below. Existing terminal
clients must be closed and reopened to load the new launcher.

## Confirmed defects and corrections

| Defect | Reproduction / evidence | Correction and replay |
| --- | --- | --- |
| A second folder replaces the shared daemon and disconnects the first window. | `baseline-native/result.json`: PID 4076257 became 4076360; the first terminal process remained alive but was no longer connected. The second window itself opened successfully on this small fixture. | Serialize startup/publication with a bounded cross-process lock. Publish workspace changes to the existing daemon; apply registration through the authority owner and replace roots, folder declarations, and effective host permissions together. All nine final cases preserved daemon identity. |
| The selected window's read-only trust changes the installation-wide tool inventory. | Source: bootstrap configuration chose `[tools].allow` from the active workspace's writable bit, making a different window change core daemon configuration. | Keep the compiled tool catalog stable and retain workspace-specific host permission decisions. Both trust directions opened successfully; `/permissions` showed the selected second window's expected effective write permission. This is permission-display and continuity coverage, not a new exhaustive permission-security campaign. |
| An ignored legacy provider timeout can obstruct an otherwise valid live workspace update. | `before-legacy-policy/result.json`: the first window survived, but the second could not open with the documented retired timeout field in an old immutable configuration. | Compare effective runtime policy with only that retired field normalized. `deliverable-legacy` passed while original configuration bytes remained unchanged. |

Implementation hardening also covers transient acknowledgment publication failure, malformed
updates, old daemons without this capability, and unavailable inactive folders. These are
replayed edge cases, not additional claims of observed production defects.

## Behavior and ownership

- The launcher owns a short-lived launch lock, immutable update publication, executable
  compatibility checks, and a bounded wait for a process-bound acknowledgment.
- A daemon-owned, shutdown-aware coordinator watches workspace updates. Status remains a
  read-only request. Closing a terminal does not own or cancel this coordinator.
- The authority owner serializes workspace registration/reconciliation. New or changed
  direct folders require current identity validation before journal mutation. Unchanged
  admitted folders can remain recorded while unavailable; effect-time identity checks remain.
- Removing a folder from recents retains its binding until restart so another window can
  continue using it. Existing bindings cannot be silently replaced. Provider, executable,
  or other core policy changes require an explicit graceful restart.
- Acknowledgment includes daemon PID and generation. After a transient publication failure,
  retry publishes the acknowledgment without repeating the completed registration change.
- Public `DaemonRuntime::start` and manually named configuration files retain static
  configuration behavior. Only launcher-generated configurations enable this coordinator.

The principal implementation owners are [launcher coordination](../../crates/app/peritus-launcher/src/daemon/catalog.rs),
[workspace snapshots](../../crates/app/peritus-daemon/src/product_run/workspace_catalog.rs),
and [authority reconciliation](../../crates/app/peritus-daemon/src/startup/workspace.rs).

## Final real-interface matrix

All cases used the final matching CLI/daemon pair described below. Maximum concurrent
fixtures: two. Setup, trust selection, permission inspection, reconnection, and closing
windows used actual Linux PTYs. No substitute daemon or model response was used.

| Evidence directory | Journey | Second ready | Outcome |
| --- | --- | ---: | --- |
| `deliverable-bare` | Bare `peritus` in two different folders with spaces and Unicode names | 2.271 s | PASS |
| `deliverable-recovery` | Force acknowledgment write failure, restore storage, retry; reject four invalid updates | 1.944 s after repair | PASS |
| `deliverable-models` | Start one real Codex planning request before opening the second folder; complete another request there | 2.110 s | PASS |
| `deliverable-legacy` | Upgrade a disposable old installation containing the retired provider timeout | 2.108 s | PASS |
| `deliverable-managed` | Two real Git repositories and their managed workspaces | 2.266 s | PASS |
| `deliverable-readonly` | Writable first folder, read-only second folder | 2.435 s | PASS |
| `deliverable-readonly-first` | Read-only first folder, writable second folder | 2.106 s | PASS |
| `deliverable-same` | Two clients selecting the same workspace | 1.950 s | PASS |
| `deliverable-missing` | Remember a spare folder, make it unavailable, open another folder, restore/reopen the spare | 2.266 s | PASS |

The model case observed overlapping provider turns. Both independent summaries contained
the correct workspace's literal canary, both requests completed, and the planning requests
left fixture files unchanged. `first_still_ready: false` in its result records that the first
window was busy at the instant the second opened; completion and subsequent connection
checks independently passed.

Each invalid-update case waited for an actual daemon rejection diagnostic, then checked
status availability, unchanged acknowledgment, unchanged daemon identity, and the first
terminal's connection: core runtime-policy change, unknown field, oversized file, symlink.

A separate `deliverable-recovery/manual-config-result.json` records successful start,
status, and graceful shutdown using `manual.toml`. Its acknowledgment remained unchanged
despite retained pending update files, demonstrating static manual configuration behavior.

## Reproduction and artifacts

Use the [replay instructions](reproducers/second-instance/README.md) and
[driver](reproducers/second-instance/replay.py). Results and terminal evidence live outside
the repository at `/home/doll/Peritus-Second-Instance-2026-10-15`:

- `baseline-native/`: copied installed pair, observed restart and lost connection.
- `before-legacy-policy/`: failing legacy-timeout replay before its correction.
- `final-results.json`: exact final commands, exit statuses, timings, and independent checks.
- The nine `deliverable-*` fixture directories: terminal screens, raw terminal bytes,
  generated ordinary configuration inputs, daemon records/logs, and result JSON.
- `final-artifact.json`: identities of the final executable pair. The immutable binary
  directory is `deliverable-bin-a124fe1b0071`.

Build: `cargo build --locked -p peritus-cli -p peritus-daemon -j 2` succeeded. Formatting
was applied only to changed Rust files. Debug information was stripped from copies of the
built executables; the final replays used those copies. CLI SHA-256:
`d9ac99e5fce4c1477b5beeafc13eca02dbc892204e7bd64694874bcc9bd6fbba`.
Daemon SHA-256:
`b34b0a00addb9722b01eb5db9fa71d9389679910f5bd4cffead913f3ab4dcafe`.

No unit tests, mock suites, formal verification, repository verification, or qualification
suites were created or run. No provider credentials were copied into the evidence files.

## Limits

This is bounded Linux CLI/TUI coverage. It does not establish native macOS/Windows
behavior, a browser UI journey, hours of uninterrupted concurrent coding, every provider,
or the full coding/reviewer/fixer pipeline. The active requests were read-only planning
tasks through actual model and workspace interfaces. An earlier attempt to request an
unavailable command tool in that mode correctly refused execution; it is not counted as
successful command-execution coverage. No live user run was retried by this campaign.

## Authorized local deployment

After the isolated campaign, the user authorized installation, restart, and resumption.
The optimized build completed with two Cargo jobs. A fresh release replay of bare launches,
reconnection, permissions display, closing the other client, and four rejected updates
passed (`release-e3f22be5f/`); second-window readiness was 0.487 seconds.

The old daemon reported clean shutdown with no remaining work. All three installed
executables were backed up and replaced atomically. The first cold start exceeded the
launcher's existing 30-second readiness window, which stopped its owned startup child.
A directly started matching packaged daemon subsequently reached read-write readiness;
its actual executable and configuration identities were recorded for launcher reuse.
The startup timeout remains a separate limitation for large accumulated state.

The exact browser run was retried once through the CLI. During its initial reacquisition
of checks, the first attempt to open the second folder hit the live-update readiness
request's timeout; the daemon and run were preserved. Retrying after that busy boundary
opened both actual terminal clients. This transient busy-state timeout is retained as
an observed limitation, rather than counted as a passing attempt.

The successful live replay retained PID 4155654 and its start token, confirmed both
clients alive without a displayed disconnect in the first, and independently read back
the browser run as Running in review with provider activity. The replay closed only its
own terminal clients; the resumed daemon and job continued. The original old terminal
client was exited gracefully so it could not reuse its old launcher implementation.

Local deployment evidence is in `install-e3f22be5f/`: previous binaries, checksum manifest,
clean shutdown response, retained run record, terminal screens, and `live-result.json`.
Installed CLI SHA-256: `ca9de9d709011df1e11aeeb06533e32bc0d71d94dc6e93e2aeea6c4f34bde72c`;
daemon: `b01ab56b16746087640cb7ee2aca000b2f41df500a9cd43ad30c84210ead6cb7`.
