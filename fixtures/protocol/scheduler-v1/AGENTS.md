# fixtures/protocol/scheduler-v1 — navigation

## Responsibility

Version-one scheduler wire/checkpoint bytes and their recorded SHA-256 inventory.

## Files in this folder

| File | Role |
| --- | --- |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |
| [scheduler-command.bin](scheduler-command.bin) | Retained binary/archive data for scheduler command; not a source-code entrypoint. |
| [scheduler-event.bin](scheduler-event.bin) | Retained binary/archive data for scheduler event; not a source-code entrypoint. |
| [scheduler-state.bin](scheduler-state.bin) | Retained binary/archive data for scheduler state; not a source-code entrypoint. |

## Working in this area

Choose the exact version and recovery disposition through the child waypoints before inspecting retained bytes. These directories organize inputs and expected observations; follow the parent map to the active codec, scheduler, or recovery implementation.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
