# crates/foundation/peritus-types/src — navigation

## Responsibility

Verified, time-independent primitive domain types for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [capability.rs](capability.rs) | Validated, hierarchical capability names. |
| [digest.rs](digest.rs) | Exact digest bytes without hashing or authenticity semantics. |
| [errors.rs](errors.rs) | Stable errors returned by checked primitive constructors and arithmetic. |
| [lib.rs](lib.rs) | Verified, time-independent primitive domain types for Peritus. |
| [numbers.rs](numbers.rs) | One-based revision, event-sequence, and generation numbers. |
| [resource.rs](resource.rs) | Resource dimensions and checked quantities. |
| [revision.rs](revision.rs) | Exact time-independent revision identity shared across authority decisions. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [ids/](ids/AGENTS.md) | Nominal identifier types grouped by domain responsibility. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
