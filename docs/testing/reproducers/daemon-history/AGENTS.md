# docs/testing/reproducers/daemon-history — navigation

## Responsibility

Real-interface replay resources for daemon history; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [probe.rs](probe.rs) | Local implementation for docs / testing / reproducers / daemon-history. Entry names: `Error`, `connect`, `request`, `main`. |
| [replay.py](replay.py) | Real terminal, daemon, and persisted history replay; no mocked application. |
| [screen.rs](screen.rs) | Local implementation for docs / testing / reproducers / daemon-history. Entry names: `main`. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
