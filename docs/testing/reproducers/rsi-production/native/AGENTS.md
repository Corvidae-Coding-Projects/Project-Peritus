# docs/testing/reproducers/rsi-production/native — navigation

## Responsibility

peritus-rsi-native-real-consumer package.

Package `peritus-rsi-native-real-consumer` is a standalone consumer/example package outside root workspace membership.

Local dependency waypoints: [peritus-daemon](../../../../../crates/app/peritus-daemon/AGENTS.md), [peritus-app-protocol](../../../../../crates/app/peritus-app-protocol/AGENTS.md), [peritus-eval](../../../../../crates/analysis/peritus-eval/AGENTS.md), [peritus-artifact-store](../../../../../crates/state/peritus-artifact-store/AGENTS.md), [peritus-approval](../../../../../crates/state/peritus-approval/AGENTS.md), [peritus-journal](../../../../../crates/state/peritus-journal/AGENTS.md), [peritus-harness](../../../../../crates/orchestration/peritus-harness/AGENTS.md), [peritus-scheduler](../../../../../crates/orchestration/peritus-scheduler/AGENTS.md), [peritus-model-protocol](../../../../../crates/model/peritus-model-protocol/AGENTS.md), [peritus-types](../../../../../crates/foundation/peritus-types/AGENTS.md), [peritus-codec](../../../../../crates/foundation/peritus-codec/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.lock](Cargo.lock) | Locked dependency resolution for the neighboring package manifest. |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-rsi-native-real-consumer`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [RESULTS-2026-10-13.md](RESULTS-2026-10-13.md) | Document: Native RSI final real-operation evidence. |
| [checkpoint_replay.py](checkpoint_replay.py) | Capture a genuine committed C0 boundary, then reopen only a disposable copied prefix. |
| [crash_replay.py](crash_replay.py) | Abruptly kill only a process this driver started, then reopen its actual daemon state. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Real-interface replay resources for src; start with the local README/driver and its documented disposable-state prerequisites. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
