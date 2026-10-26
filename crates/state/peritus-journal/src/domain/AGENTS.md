# crates/state/peritus-journal/src/domain — navigation

## Responsibility

Typed B0/B1 durable transition adapters.

## Files in this folder

| File | Role |
| --- | --- |
| [approval.rs](approval.rs) | Commit-once human-approval transition persistence. |
| [approval_use.rs](approval_use.rs) | Atomic durable approve-once consumption persistence. |
| [budget.rs](budget.rs) | Commit-once budget transitions and durable non-activation observations. |
| [capability.rs](capability.rs) | Commit-once capability-use persistence. |
| [commit.rs](commit.rs) | Shared post-commit state observation path. |
| [encoding.rs](encoding.rs) | Small canonical encoder for journal-owned transition state values. |
| [kernel.rs](kernel.rs) | B0 durable-transition adapter and reducer-driven restart replay. |
| [lease.rs](lease.rs) | Commit-once lease compare-and-swap persistence. |
| [mod.rs](mod.rs) | Typed B0/B1 durable transition adapters. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [approval_use/](approval_use/AGENTS.md) | Implementation pieces for `approval_use.rs`: Atomic durable approve-once consumption persistence. |
| [kernel/](kernel/AGENTS.md) | Implementation pieces for `kernel.rs`: B0 durable-transition adapter and reducer-driven restart replay. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
