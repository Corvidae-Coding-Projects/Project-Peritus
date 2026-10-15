# docs/testing/reproducers/computer-automation — navigation

## Responsibility

Real-interface replay resources for computer automation; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [replay-native-menu.py](replay-native-menu.py) | Open and observe a real KWrite File popup on its isolated display. |
| [replay-native-text.py](replay-native-text.py) | Replay confirmed defects through real KWrite/Zenity windows and the shipped driver. No unit framework or substituted application. Requires Linux native dependencies. |
| [replay-owner-exit.py](replay-owner-exit.py) | Run real native apps; owner exit must close owned apps and preserve attached apps. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
