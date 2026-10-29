# docs/testing/reproducers/rsi-production — navigation

## Responsibility

Real-interface replay resources for rsi production; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [legacy_replay.py](legacy_replay.py) | Reopen a copied historical failed E3 schedule with the actual daemon and CLI. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [evolution/](evolution/AGENTS.md) | peritus-rsi-evolution-replay package. |
| [native/](native/AGENTS.md) | peritus-rsi-native-real-consumer package. |
| [recovery/](recovery/AGENTS.md) | Real-interface replay resources for recovery; start with the local README/driver and its documented disposable-state prerequisites. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
