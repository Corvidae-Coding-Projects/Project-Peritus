# crates/runtime/peritus-git/src/snapshot — navigation

## Responsibility

Implementation pieces for `snapshot.rs`: Candidate-tree creation, retained snapshots, and exact tree restoration.

## Files in this folder

| File | Role |
| --- | --- |
| [nested.rs](nested.rs) | Explicit boundaries for independently owned nested repositories. |
| [operations.rs](operations.rs) | Git effects for candidates, snapshot references, and restoration. |
| [quarantine.rs](quarantine.rs) | Atomic containment for divergent retained snapshot references. |
| [support.rs](support.rs) | Canonical snapshot identities, manifests, and reference compare-and-swap. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [support/](support/AGENTS.md) | Implementation pieces for `support.rs`: Canonical snapshot identities, manifests, and reference compare-and-swap. |

Owning module: [snapshot.rs](../snapshot.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
