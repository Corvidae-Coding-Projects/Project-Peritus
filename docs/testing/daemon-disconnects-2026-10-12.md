# Long-history daemon stalls and disappearing disconnect diagnostics

The reported symptom was a silent loss of the terminal connection requiring Ctrl-R. The live daemon remained alive for more than four hours, answered fresh CLI status requests, used roughly one CPU core, and had no new heartbeat mismatch. Its current log contained broken-pipe errors when old connections closed. Those errors identify the failed server write, not the client's original decision to close.

No unit tests, mocked services, formal verification or repository verification suites ran. The campaign builds and operates actual Peritus executables, a real TUI, real authenticated IPC clients, real SQLite databases and copied durable histories. All fixture processes and native process ownership are isolated from the user's running browser task.

## Confirmed defects

### DH-001: full immutable conversation replay on every live read and append

A copy of the active conversation had 1,939 control revisions and a 752,832,512-byte control database. Each binding request traversed the complete immutable history repeatedly, including archived request and checkpoint evidence. The original installed daemon took 3,010–3,104 ms per binding request in five consecutive real IPC exchanges. An idle copied daemon with its TUI polling used about 96% of one CPU core, with no provider task running.

`ControlStore::load` rebuilt the entire conversation each time. `synchronize_public_inputs` held the run registry write lock throughout that replay and cloned the complete run even when there was no new public input. Provider activity and other client reads compete for the same owners, so repeated history processing creates long-run stalls and increasing timeout pressure.

The repair retains one fully verified conversation replay in memory. The existing journal `ReplayObservation` invalidates it after another append attempt or any external SQLite commit. Cold loading still checks the full event chain, exact archive artifacts and current state root. Successful guarded appends retain their successor only after resolving the exact durable receipt and checking its new archive. Fork publication invalidates the observation through the existing journal append generation. Exact historical-revision reads still use full replay; no persisted cache, migration or new authority token is introduced.

Public input synchronization reads the control history outside the run registry lock, then applies only newly observed input under the write lock. It skips cloning/persistence when unchanged and preserves newer input already published by concurrent synchronization.

### DH-002: the conversation screen polls hidden run panels and duplicates interaction reads

Every second the TUI requested the chat interaction, the recent run list, the selected run observation and the selected interaction again. The expanded fixture contains all 13 run records and all 17 effect ledgers, totaling about 21.7 MB of receipts. Those receipts contain retained tool output and were repeatedly decoded even while their panels were hidden.

With only DH-001 repaired, this fuller fixture still used 22.33% of one CPU core while displaying chat. The repair returns after chat polling when the conversation screen is visible. A chat snapshot already updates the corresponding run and settlement; opening the run panels resumes their existing inspection on the next tick. Visible panel behavior is retained and exercised through `/runs` and Esc.

### DH-003: disconnect details disappear after six seconds

`Action::Disconnected` puts the actual failure in a transient notice. The chat footer previously rendered an empty second line once that notice expired, while retaining only the generic Offline label and Ctrl-R hint. An unattended user could return to an apparently silent disconnect.

The repair renders the retained connection failure when no transient notice remains. The actual TUI was disconnected by graceful shutdown of its owned daemon; ten seconds later the screen still displayed `Offline` and `I/O failure: early eof`. This is observed terminal rendering, not a model-level assertion.

## Behavioral evidence

| Real workflow | Independent expectation and observation |
| --- | --- |
| Five consecutive binding reads, original daemon | Full-history stall reproduced: 3,010–3,104 ms each. |
| Binding reads, repaired daemon, isolated active record | Same run/conversation/activity routing; warm reads 0.84–2.62 ms. |
| All 13 records and 17 effect ledgers | Same public binding and received-input count; warm reads 51.39–58.15 ms, including actual command-receipt inspection. |
| 36 generated alternating pin operations | Each advances exactly once; original-operation replay returns the same receipt; a different stale operation is rejected without changing state; original pinning restored. Fresh clients interleaved. |
| Eight distinct queued inputs | Unicode, newline and tab bytes accepted through public queue commands appear exactly in the public conversation activity. Acceptance starts no inference. |
| External SQLite corruption after warming cache | Damaged current root bytes are rejected with `state value digest does not match exact bytes`. Cache does not hide the commit. Restoring exact bytes permits a fresh cold replay; subsequent reads become fast again. |
| Actual terminal reconnect and daemon restart | Unsent draft survives Ctrl-R, graceful shutdown and a fresh daemon process; retained history reopens without starting inference. |
| Ordinary and delayed heartbeats | Sustained polling stays connected; a 17-second pause of only the owned CLI resumes and remains connected. |
| Persistent disconnect reason | Actual EOF remains visible ten seconds after shutdown, beyond the six-second notice lifetime. |
| Visible run panels | `/runs` opens the real run dashboard and Esc returns to the same conversation. |

The native TUI campaign was expanded from the active record to all stored records and effect ledgers. Its final result, CPU sampling, terminal captures, decoded terminal screen snapshots, process identities and shutdown observations are retained in `/home/doll/Peritus-disconnects-2026-10-12/candidate/`. Historical private data stays outside Git.

## Build and replay

```sh
cargo build --locked --release -p peritus-daemon -j 2
cargo build --locked --release -p peritus-cli -p peritus-tui -j 2
```

The saved [real replay](reproducers/daemon-history/README.md) includes a public IPC consumer, terminal screen decoder and native TUI action driver. The original installed executables are retained in the baseline fixture and before-local-update directory. Builds were limited to two jobs; the fixtures copied no native process registry and launched no background AI work.

## Scope and remaining observation

The reproductions establish the history-processing stalls, unnecessary hidden polling, disappearing diagnostic and their repairs. The original intermittent socket-close decision was not captured with its precise client error, so this report does not claim that every possible future disconnect has been eliminated. Cold startup and explicit historical reads still perform full integrity replay. Windows and macOS behavior and a new multi-hour run are not covered by this Linux campaign. The ongoing browser run supplies separate live deployment evidence below.

## Final local deployment

The final 125.69-second TUI replay passed with all 13 records and 17 effect ledgers. Chat polling used 5.93% of one CPU core after both history and hidden-panel repairs, down from 22.33% with only history caching. It also exercised the visible Runs panel and return to chat, unsent draft retention, delayed heartbeat, persistent EOF diagnostic and restart.

The three rebuilt executables were installed atomically in `/home/doll/.local/bin`, retaining previous binaries and the run record in the private before-local-update directory. Public graceful shutdown saved the original browser run and reported clean completion. The retired daemon subsequently kept one blocking control-database reader alive before finally exiting and releasing its store ownership; the replacement was started only after that exit, without killing user processes. This observed retirement delay is recorded separately from the verified polling improvements.

The replacement daemon is PID 921075, SHA-256 `b176f0aa7c0ef122f9b544904d1d1964f12c2cf036df40aff1ae219563bb40b7`, and returns `ready-read-write`. The same run and workspace IDs were retried through public CLI controls, preserving the browser files, durable inputs and checkpoints. During the first three minutes it advanced from 1,112 requests / 2,565 tools to 1,121 / 2,580. Five real live binding reads took 25.13–30.47 ms; daemon CPU averaged about 5.9%. No new connection error from this PID was observed in that interval. The continuing run performs its normal recovery design refresh.

The user's preexisting terminal process still maps its old embedded UI binary. Ctrl-Q followed by reopening the same run loads the installed client changes; a draft must be saved first because process-local drafts are retained across reconnect, not across client process replacement. The browser task continues independently in the daemon. Both owned fixture daemons were shut down cleanly. This is a bounded observation after deployment, not a claim of new multi-hour coverage.

At eight minutes after restart, the live run was back in Writing and had reached 1,134 requests / 2,605 tool calls with no interruption cause. The daemon averaged 5.8% CPU and used about 260 MiB RSS while executing the real provider-backed task. Its new PID had emitted no connection error. The user's original run identity, workspace identity and eleven accepted input generations were preserved; disposable fixture queue edits remained isolated.

The user restarted her terminal client. Its new PID 931864 maps exactly the installed CLI SHA-256 `b43520c118e49898df68afe4c1ea6641f26f0fcbea7ea6f5bc18c3e5bb5a3126`, so both TUI repairs are active there. The inspection-only mirror TUI and final fixture TUI were closed with Ctrl-Q; the final owned fixture daemon was shut down through public CLI controls. The live browser task was left running in its daemon.
