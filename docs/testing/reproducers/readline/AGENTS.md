# docs/testing/reproducers/readline — navigation

## Responsibility

Real-interface replay resources for readline; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [regressions.json](regressions.json) | Structured regressions data with fields `issue`, `seed`, `regressions`. |
| [replay.py](replay.py) | Operate actual Peritus via PTY, decode its screen, and replay human editing. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
