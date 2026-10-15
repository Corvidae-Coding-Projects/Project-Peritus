# docs/testing/reproducers/rsi-production/native/src — navigation

## Responsibility

Real-interface replay resources for src; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [evaluator.py](evaluator.py) | Actual-interface replay/setup driver for evaluator. |
| [main.rs](main.rs) | Actual public daemon/evaluation consumer. No unit suite or synthetic execution observations. Entry names: `Result`, `id`, `helper_path`, `cgroup_root`, `hex`. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
