# docs/testing/reproducers/runtime-friction/command-output — navigation

## Responsibility

Real-interface replay resources for command output; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [build-consumer.py](build-consumer.py) | Compile a normal public-API consumer against the already-built Peritus copy. |
| [fixture_gen.py](fixture_gen.py) | Deterministic stdout fixture for an actual Peritus command, with bounded working memory. |
| [replay.rs](replay.rs) | A normal executable consumer of Peritus's public command API; no test harness. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
