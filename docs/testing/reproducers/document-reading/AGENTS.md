# docs/testing/reproducers/document-reading — navigation

## Responsibility

Real-interface replay resources for document reading; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [check-replay.py](check-replay.py) | Check real Peritus trace frames and model receipts against generated source files. |
| [prepare-fixtures.py](prepare-fixtures.py) | Generate ordinary text fixtures and a separate oracle for real Peritus reads. |

## Working in this area

Start with the listed driver and its nearest campaign report to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
