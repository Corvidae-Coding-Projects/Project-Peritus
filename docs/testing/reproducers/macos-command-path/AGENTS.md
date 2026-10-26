# docs/testing/reproducers/macos-command-path — navigation

## Responsibility

Real-interface replay resources for macos command path; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [consumer.rs](consumer.rs) | Real consumer of the production packaged-binary discovery API. |
| [install_replay.py](install_replay.py) | Actual shell installers and repair command in a mount-isolated user home. |
| [replay.py](replay.py) | Operate real packaged-binary discovery against symlinked command paths. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
