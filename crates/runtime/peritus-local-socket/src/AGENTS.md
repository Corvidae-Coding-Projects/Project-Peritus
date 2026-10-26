# crates/runtime/peritus-local-socket/src — navigation

## Responsibility

Bounded, owner-protected paths for ordinary Unix-domain sockets.

## Files in this folder

| File | Role |
| --- | --- |
| [lib.rs](lib.rs) | Bounded, owner-protected paths for ordinary Unix-domain sockets. |
| [path.rs](path.rs) | Deterministic endpoint addressing, also usable by cross-platform qualification plans. |
| [unix.rs](unix.rs) | Owner-checked custody of the private runtime directory used by an oversized endpoint. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `path_fits`, `private_directory`, `protected_temporary_root`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [unix/](unix/AGENTS.md) | Implementation pieces for `unix.rs`: Owner-checked custody of the private runtime directory used by an oversized endpoint. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
