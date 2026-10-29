# docs/testing/reproducers/rsi-inbox — navigation

## Responsibility

Real-interface replay resources for rsi inbox; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [build_observer.py](build_observer.py) | Build a public IPC consumer which reads the actual evaluation input queue. |
| [check_directive.py](check_directive.py) | Read the daemon's real workbench queue; reject the old acceptance-policy mismatch. |
| [check_no_patch.py](check_no_patch.py) | Inspect a real completed no-patch evaluation and replay its refused controls. |
| [observer.rs](observer.rs) | Local implementation for docs / testing / reproducers / rsi-inbox. |
| [plan.rs](plan.rs) | Actual consumer of the candidate plan discovered from a real checked-out workspace. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
