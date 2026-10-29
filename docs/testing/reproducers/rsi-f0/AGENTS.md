# docs/testing/reproducers/rsi-f0 — navigation

## Responsibility

Real-interface replay resources for rsi f0; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [build_consumer.py](build_consumer.py) | Build the real public E3/F0 consumer against this checkout, with two jobs. |
| [consumer.rs](consumer.rs) | Local implementation for docs / testing / reproducers / rsi-f0. Entry names: `AnyResult`, `digest`, `eid`, `save`, `read`. |
| [evaluator.py](evaluator.py) | Independent CLI requirements oracle, run only in evaluator stage. |
| [replay.py](replay.py) | Replay the real E3-to-daemon candidate scheduling failure. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
