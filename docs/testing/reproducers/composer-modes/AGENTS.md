# docs/testing/reproducers/composer-modes — navigation

## Responsibility

Real-interface replay resources for composer modes; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [REPORT-2026-10-13.md](REPORT-2026-10-13.md) | Document: Composer mode capability replay — GitHub #122. |
| [SHARED-CALLERS-REPORT-2026-10-13.md](SHARED-CALLERS-REPORT-2026-10-13.md) | Document: Shared editor routing — actual UI replay. |
| [keyboard-regressions.json](keyboard-regressions.json) | Structured keyboard regressions data with fields `interface`, `baseline_sha256`, `fixed_sha256`, `cases`. |
| [replay-webui.mjs](replay-webui.mjs) | Browser client module for replay webui. |
| [shared_callers_replay.py](shared_callers_replay.py) | Replay shared-editor routing through real Peritus/daemon/PTY interfaces. |
| [tui_replay.py](tui_replay.py) | Operate actual Peritus via PTY, decode its screen, and replay human editing. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
