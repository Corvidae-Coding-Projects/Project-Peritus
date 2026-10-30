# Accepted failure replay manifests

## Folder guide

Retained drivers/consumers for replaying actual program defects with disposable state and independently stated expectations.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [web-reader-recovery.py](web-reader-recovery.py) | Real terminal/daemon/provider replay of a wedged page followed by fresh-reader recovery. |
| [web-search-native.py](web-search-native.py) | Live built-in search with empty PATH, official source reads, fresh-process cache and migration/recovery. |
| [web-search-consumer.rs](web-search-consumer.rs) | Actual public metasearch requests, invalid inputs, active/waiting cancellation and recovery. |
| [web-search-daemon.py](web-search-daemon.py) | Actual terminal/daemon/provider built-in search followed by primary source reading. |
| [web-tools.py](web-tools.py) | Native web CLI and HTTP document campaign; see docs/web-tools.md for feature use. |
| [web-browser.py](web-browser.py) | Real installed Chrome/Playwright Chromium and controller replay. |
| [web-native-consumer.rs](web-native-consumer.rs) | Public web API cancellation and recovery consumer. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [reviewer-startup.py](reviewer-startup.py) | Real provider-settings PTY replay for reviewer selection, retention, reset, invalid input, and cancellation. |
| [cancellation-process-death.json](cancellation-process-death.json) | Structured cancellation process death data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [cancellation-race.json](cancellation-race.json) | Structured cancellation race data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [command-ordinal-contention.json](command-ordinal-contention.json) | Structured command ordinal contention data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [framing-split-crlf.json](framing-split-crlf.json) | Structured framing split crlf data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [fuzz-target-ci-entrypoints.json](fuzz-target-ci-entrypoints.json) | Structured fuzz target ci entrypoints data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [posix-supervisor-failure.json](posix-supervisor-failure.json) | Structured posix supervisor failure data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [receipt-crash-tail.json](receipt-crash-tail.json) | Structured receipt crash tail data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [repeated-recovery-narration.json](repeated-recovery-narration.json) | Structured repeated recovery narration data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [reused-provider-call-id.json](reused-provider-call-id.json) | Structured reused provider call id data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [tui-reconnect/](tui-reconnect/AGENTS.md) | Actual terminal/daemon loss, automatic reconnection, draft retention and optional real-provider Retry replay. |
| [composer-modes/](composer-modes/AGENTS.md) | Real-interface replay resources for composer modes; start with the local README/driver and its documented disposable-state prerequisites. |
| [computer-automation/](computer-automation/AGENTS.md) | Real-interface replay resources for computer automation; start with the local README/driver and its documented disposable-state prerequisites. |
| [context-resume/](context-resume/AGENTS.md) | Real-interface replay resources for context resume; start with the local README/driver and its documented disposable-state prerequisites. |
| [daemon-history/](daemon-history/AGENTS.md) | Real-interface replay resources for daemon history; start with the local README/driver and its documented disposable-state prerequisites. |
| [document-reading/](document-reading/AGENTS.md) | Real-interface replay resources for document reading; start with the local README/driver and its documented disposable-state prerequisites. |
| [folder-git-reopen/](folder-git-reopen/AGENTS.md) | Real-interface replay resources for folder git reopen; start with the local README/driver and its documented disposable-state prerequisites. |
| [history-recovery/](history-recovery/AGENTS.md) | Real-interface replay resources for history recovery; start with the local README/driver and its documented disposable-state prerequisites. |
| [in-place-scope/](in-place-scope/AGENTS.md) | Real-interface replay resources for in place scope; start with the local README/driver and its documented disposable-state prerequisites. |
| [macos-command-path/](macos-command-path/AGENTS.md) | Real-interface replay resources for macos command path; start with the local README/driver and its documented disposable-state prerequisites. |
| [opencode-quoted-report/](opencode-quoted-report/AGENTS.md) | Real-interface replay resources for opencode quoted report; start with the local README/driver and its documented disposable-state prerequisites. |
| [prompt-skills/](prompt-skills/AGENTS.md) | Real-interface replay resources for prompt skills; start with the local README/driver and its documented disposable-state prerequisites. |
| [readline/](readline/AGENTS.md) | Real-interface replay resources for readline; start with the local README/driver and its documented disposable-state prerequisites. |
| [rsi-f0/](rsi-f0/AGENTS.md) | Real-interface replay resources for rsi f0; start with the local README/driver and its documented disposable-state prerequisites. |
| [rsi-inbox/](rsi-inbox/AGENTS.md) | Real-interface replay resources for rsi inbox; start with the local README/driver and its documented disposable-state prerequisites. |
| [rsi-production/](rsi-production/AGENTS.md) | Real-interface replay resources for rsi production; start with the local README/driver and its documented disposable-state prerequisites. |
| [runtime-friction/](runtime-friction/AGENTS.md) | Real-interface replay resources for runtime friction; start with the local README/driver and its documented disposable-state prerequisites. |
| [updater-release-metadata/](updater-release-metadata/AGENTS.md) | Real-interface replay resources for updater release metadata; start with the local README/driver and its documented disposable-state prerequisites. |

Each JSON file is a schema-versioned replay record for one accepted product defect. It binds the
finding to the original revision, a minimized deterministic schedule, the required boundary hit,
the logical failure signature, three fresh-fixture reproductions, the owning regression, and the
first fixing revision. At least three fresh fixtures are required; a manifest may record more.
Paths are relative to the repository root and commands run from that root.

Schema version 1 requires `id`, `classification`, `invariant`, `original`, `reproducer`, `fix`,
`fixed_replay`, `negative_control`, `cleanup`, and `limitations`. The `original` object records the
source revision and observed failure; `reproducer` records the exact test and reachability; and
`fixed_replay` records the expected post-fix result. These manifests summarize checked-in tests and
evidence documents; they do not replace the owning regressions.

See [module-foundation.md](module-foundation.md) for the module registry, public API,
terminal/browser/provider, cancellation, role-isolation and restart replays. All
execution uses disposable state and real program interfaces.

[Web recovery](web-recovery/README.md) covers public Store consumers, exact source paging, failure/cooldown classification, real browser navigation, default daemon recovery and explicitly selected HTTPS sources.
