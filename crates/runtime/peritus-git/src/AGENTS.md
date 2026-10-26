# crates/runtime/peritus-git/src — navigation

## Responsibility

Structured Git repository and detached-worktree operations for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [baseline.rs](baseline.rs) | One-time resolution of immutable commit/tree baselines. |
| [cancellation.rs](cancellation.rs) | Explicit cancellation shared by one owned Git operation and its controller. |
| [command.rs](command.rs) | Narrow shell-free Git subprocess boundary. |
| [diff.rs](diff.rs) | Bounded structured diff between two immutable commits. |
| [error.rs](error.rs) | Stable Git adapter failures and recovery guidance. |
| [history.rs](history.rs) | Bounded structured commit history from one immutable start commit. |
| [lib.rs](lib.rs) | Structured Git repository and detached-worktree operations for Peritus. |
| [manifest.rs](manifest.rs) | Canonical, versioned persistence records for restart-safe Git handles. |
| [name.rs](name.rs) | Safe names used to derive managed worktree paths. |
| [object_id.rs](object_id.rs) | Algorithm-tagged Git object identifiers. |
| [reconcile.rs](reconcile.rs) | Deterministic reconciliation of exact Git observations with expected state. |
| [repository.rs](repository.rs) | Repository discovery and stable identity binding. |
| [snapshot.rs](snapshot.rs) | Candidate-tree creation, retained snapshots, and exact tree restoration. |
| [status.rs](status.rs) | Bounded typed observations of Git porcelain-v2 status. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `supported_object_hex_length`, `worktree_name_byte_allowed`, `reconciliation_is_clean`. |
| [worktree.rs](worktree.rs) | Detached linked-worktree registrations and lifecycle operations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Narrow shell-free Git subprocess boundary. |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Canonical, versioned persistence records for restart-safe Git handles. |
| [snapshot/](snapshot/AGENTS.md) | Implementation pieces for `snapshot.rs`: Candidate-tree creation, retained snapshots, and exact tree restoration. |
| [status/](status/AGENTS.md) | Implementation pieces for `status.rs`: Bounded typed observations of Git porcelain-v2 status. |
| [worktree/](worktree/AGENTS.md) | Implementation pieces for `worktree.rs`: Detached linked-worktree registrations and lifecycle operations. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
