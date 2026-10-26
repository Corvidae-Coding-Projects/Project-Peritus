# fixtures/protocol/scheduler-v1-recovery — navigation

## Responsibility

Version-one scheduler recovery inputs grouped into benign replay, loss, and retry cases; digests bind the retained bytes.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [benign/](benign/AGENTS.md) | Retained scheduler checkpoint and event bytes for benign recovery/replay. |
| [loss/](loss/AGENTS.md) | Retained scheduler checkpoint and event bytes representing loss during recovery. |
| [retry/](retry/AGENTS.md) | Retained scheduler checkpoint and event bytes for interrupted work eligible for retry. |

## Working in this area

Choose the exact version and recovery disposition through the child waypoints before inspecting retained bytes. These directories organize inputs and expected observations; follow the parent map to the active codec, scheduler, or recovery implementation.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
