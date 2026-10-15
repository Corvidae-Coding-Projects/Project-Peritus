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

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
