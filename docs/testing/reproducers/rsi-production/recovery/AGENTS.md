# docs/testing/reproducers/rsi-production/recovery — navigation

## Responsibility

Real-interface replay resources for recovery; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [build_consumer.py](build_consumer.py) | Build the real public E3/F0 consumer against this checkout, with two jobs. |
| [consumer.rs](consumer.rs) | Local implementation for docs / testing / reproducers / rsi-production / recovery. Entry names: `AnyResult`, `digest`, `eid`, `save`, `read`. |
| [evaluator.py](evaluator.py) | Independent CLI requirements oracle, run only in evaluator stage. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
