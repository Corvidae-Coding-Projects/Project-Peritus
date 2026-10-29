# fixtures/protocol — navigation

## Responsibility

Scheduler protocol/checkpoint input bytes grouped by version and recovery disposition.

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [scheduler-v1/](scheduler-v1/AGENTS.md) | Version-one scheduler wire/checkpoint bytes and their recorded SHA-256 inventory. |
| [scheduler-v1-recovery/](scheduler-v1-recovery/AGENTS.md) | Version-one scheduler recovery inputs grouped into benign replay, loss, and retry cases; digests bind the retained bytes. |
| [scheduler-v2/](scheduler-v2/AGENTS.md) | Version-two scheduler wire/checkpoint inputs and recorded digests. |

## Working in this area

Choose the exact version and recovery disposition through the child waypoints before inspecting retained bytes. These directories organize inputs and expected observations; follow the parent map to the active codec, scheduler, or recovery implementation.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
