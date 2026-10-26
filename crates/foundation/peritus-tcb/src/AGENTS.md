# crates/foundation/peritus-tcb/src — navigation

## Responsibility

Audited policy anchor for the trust boundary. It exposes stable repository-relative names for actor, trusted-construct, exclusion, obligation, and proof-impact manifests, while leaving manifest loading and proof execution to external owners.

## Files in this folder

| File | Role |
| --- | --- |
| [lib.rs](lib.rs) | `no_std` public surface for the audited policy anchor and its manifest-path exports. |
| [manifests.rs](manifests.rs) | Stable repository-relative paths for the five declared trust-boundary manifest classes; it does not read, validate, or execute them. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
