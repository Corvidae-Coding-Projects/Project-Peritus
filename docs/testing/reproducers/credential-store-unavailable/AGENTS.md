# docs/testing/reproducers/credential-store-unavailable — navigation

## Responsibility

Real-interface replay resources for provider setup when the operating-system credential store is
unreachable; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [drive.py](drive.py) | pty driver that answers scripted prompts and records exact terminal output. |
| [replay.py](replay.py) | Provider-settings scenarios for an absent and a reachable credential store, with retained transcripts and an independent Secret Service inventory. |
| [secret-service-inventory.sh](secret-service-inventory.sh) | Lists and optionally removes Peritus provider items in the session Secret Service. |
| [observed-baseline.txt](observed-baseline.txt) | Verbatim transcripts of the recorded run and its check outcomes. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
