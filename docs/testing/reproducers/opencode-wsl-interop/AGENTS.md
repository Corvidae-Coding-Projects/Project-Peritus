# docs/testing/reproducers/opencode-wsl-interop — navigation

## Responsibility

Real-interface replay of OpenCode runtime selection on a WSL host: the production discovery and
catalog entry points, an independent native-host oracle, and the retained baseline observations.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Prerequisites, replay command, expected pre-fix/fixed results, and evidence boundary. |
| [replay.py](replay.py) | Creates an owned external consumer project, builds with two jobs, and records the pinned runtime and catalog outcome. |
| [consumer.rs](consumer.rs) | Calls the production discovery/catalog entry points and classifies the pinned executable against an independent native-host oracle. |
| [observed-baseline.txt](observed-baseline.txt) | Actual pre-fix path probes, isolated vendor command failure, and the reported provider-setup failure. |

Implementation owner: [OpenCode adapter](../../../../crates/model/peritus-provider-opencode/src/AGENTS.md).
Campaign: [WSL interop report](../../opencode-wsl-interop-real-testing-2026-10-20.md).
Navigation: [Parent](../AGENTS.md) · [Repository](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
