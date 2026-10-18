# docs/testing/reproducers — navigation

## Responsibility

Retained drivers/consumers for replaying actual program defects with disposable state and independently stated expectations.

## Files in this folder

| File | Role |
| --- | --- |
| [automation-apparmor.py](automation-apparmor.py) | Disposable Ubuntu public setup consumer and real Chromium sandbox/HTTP/JavaScript replay, including literal paths, opt-out and policy recovery. |
| [module-market-menu.py](module-market-menu.py) | Fresh TUI client invoking plugin-supplied numeric plan/report actions on a retained checked job. |
| [module-market-browser.mjs](module-market-browser.mjs) | Actual GUI market selection and plugin-supplied plan/Start/report control replay. |
| [native-research-rpc.py](native-research-rpc.py) | Actual native Rust executable IPC, stage correction, source checking and fresh-process report readback. |
| [market-research-consumer.md](market-research-consumer.md) | Consumer build/replay instructions and domain-only evidence boundary. |
| [market-research-consumer.rs](market-research-consumer.rs) | Independent public API/generated ledger and catalog replay. |
| [module-market-live.py](module-market-live.py) | Actual terminal market/research/provider/restart journey. |
| [module-foundation.md](module-foundation.md) | Module campaign setup, real consumers and evidence boundaries. |
| [module-registry.py](module-registry.py) | Actual CLI installation, update, repair, corruption, parallel updates and invalid package replays. |
| [module-foundation-consumer.rs](module-foundation-consumer.rs) | Public module job consumer plus real framed executable plugin and process cleanup observations. |
| [module-daemon.py](module-daemon.py) | TUI/provider four-stage workflow, separate ordinary/Git-managed fixtures, stop and daemon restart. |
| [module-resume-existing.py](module-resume-existing.py) | Reattach a retained disposable job and check first-submit resume, mode and tool permission boundaries. |
| [module-steering.py](module-steering.py) | Actual in-flight steering, uncommitted superseded stage, explicit resume and ordinary Chat. |
| [module-browser.mjs](module-browser.mjs) | Actual browser composer, registry replies and installed resource reader workflow. |
| [web-reader-recovery.py](web-reader-recovery.py) | Real terminal/daemon/provider replay of a wedged page followed by fresh-reader recovery. |
| [web-search-native.py](web-search-native.py) | Live built-in search with empty PATH, official source reads, fresh-process cache and migration/recovery. |
| [web-search-consumer.rs](web-search-consumer.rs) | Actual public metasearch requests, invalid inputs, active/waiting cancellation and recovery. |
| [web-search-daemon.py](web-search-daemon.py) | Actual terminal/daemon/provider built-in search followed by primary source reading. |
| [web-tools.py](web-tools.py) | Generated real HTTP/CLI source reads, paging, refresh, encoding and rejection journeys. |
| [web-browser.py](web-browser.py) | Actual browser/controller rendering, file rejection and independent session preservation. |
| [web-native-consumer.rs](web-native-consumer.rs) | Public-library consumer exercising real transfer cancellation and recovery. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [reviewer-startup.py](reviewer-startup.py) | Actual provider-settings PTY replay for reviewer selection, retention, invalid input, reset, and cancellation. |
| [cancellation-process-death.json](cancellation-process-death.json) | Structured cancellation process death data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [cancellation-race.json](cancellation-race.json) | Structured cancellation race data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [command-ordinal-contention.json](command-ordinal-contention.json) | Structured command ordinal contention data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [framing-split-crlf.json](framing-split-crlf.json) | Structured framing split crlf data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [fuzz-target-ci-entrypoints.json](fuzz-target-ci-entrypoints.json) | Structured fuzz target ci entrypoints data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [posix-supervisor-failure.json](posix-supervisor-failure.json) | Structured posix supervisor failure data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [receipt-crash-tail.json](receipt-crash-tail.json) | Structured receipt crash tail data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [repeated-recovery-narration.json](repeated-recovery-narration.json) | Structured repeated recovery narration data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |
| [reused-provider-call-id.json](reused-provider-call-id.json) | Structured reused provider call id data with fields `schema_version`, `id`, `classification`, `invariant`, `original`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [deep-research-inquiry/](deep-research-inquiry/AGENTS.md) | Actual native inquiry/provenance consumers and separate live-artifact readback, with research-quality judgment kept distinct. |
| [token-budgets/](token-budgets/AGENTS.md) | Actual public context/host consumers for token-budget removal, persistence, role isolation, and copied long-run assembly. |
| [binary-evidence/](binary-evidence/AGENTS.md) | Real public-API and full TUI/daemon replay of binary previews, exact historical evidence paging, and post-writer model inputs. |
| [startup-recovery/](startup-recovery/AGENTS.md) | Actual slow-start, interrupted endpoint, folder replacement, trust repair and legacy-upgrade replay. |
| [tui-reconnect/](tui-reconnect/AGENTS.md) | Actual terminal/daemon loss, automatic reconnection, draft retention and optional real-provider Retry replay. |
| [guided-design/](guided-design/AGENTS.md) | Real interview, durable Markdown, restart, corrected-choice handoff, and CLI behavior replay. |
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
| [second-instance/](second-instance/AGENTS.md) | Actual concurrent terminal windows, live catalog publication, permission readback, reconnect, and rejected-update replay. |
| [opencode-malformed-output/](opencode-malformed-output/AGENTS.md) | Real vendor inference, malformed-turn correction, exact host-read/report oracles, and saved failing vendor frames for issue 127. |
| [updater-release-metadata/](updater-release-metadata/AGENTS.md) | Real-interface replay resources for updater release metadata; start with the local README/driver and its documented disposable-state prerequisites. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.

- [retained-state-growth/](retained-state-growth/AGENTS.md): real retained-state, protocol, scope, trace and native memory growth replays.
