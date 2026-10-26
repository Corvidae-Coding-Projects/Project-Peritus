# docs/testing/reproducers/computer-automation — navigation

## Responsibility

Real-interface replay resources for computer automation; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [replay-native-menu.py](replay-native-menu.py) | Open and observe a real KWrite File popup on its isolated display. |
| [replay-native-text.py](replay-native-text.py) | Replay confirmed defects through real KWrite/Zenity windows and the shipped driver. No unit framework or substituted application. Requires Linux native dependencies. |
| [replay-owner-exit.py](replay-owner-exit.py) | Run real native apps; owner exit must close owned apps and preserve attached apps. |

## Working in this area

Start with the listed driver and its nearest campaign report to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
