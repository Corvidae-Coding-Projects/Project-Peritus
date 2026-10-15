# docs/testing/reproducers/history-recovery — navigation

## Responsibility

Real-interface replay resources for history recovery; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [check-context.py](check-context.py) | Check real reader observations, exact retrieval ranges and independent marker receipts. |
| [check-history.py](check-history.py) | Read back an actual daemon-written continuation against independent fixture data. |
| [prepare-legacy.py](prepare-legacy.py) | Generate a large legal history from an actual disposable run and its control store. |
| [replay-integrity.py](replay-integrity.py) | Restart an actual disposable daemon against valid, corrupt and missing continuations. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
