# crates/orchestration/peritus-run-knowledge/src — navigation

## Responsibility

Digest-bound run knowledge and role-delta planning for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Provenance binding shared by every retained knowledge section. |
| [change.rs](change.rs) | Current observations and explicit invalidation events. |
| [delta.rs](delta.rs) | Role-specific changed-fact and current-reference packet planning. |
| [error.rs](error.rs) | Stable checked-construction and planning failures. |
| [identity.rs](identity.rs) | Stable caller-supplied knowledge identities. |
| [kind.rs](kind.rs) | Typed run-knowledge categories and their evidence authority. |
| [lib.rs](lib.rs) | Digest-bound run knowledge and role-delta planning for Peritus. |
| [limits.rs](limits.rs) | Explicit allocation bounds for run knowledge. |
| [model.rs](model.rs) | Input-defined logical model for run-knowledge freshness and reuse. |
| [plan.rs](plan.rs) | Pure fail-closed invalidation and reuse planning. |
| [section.rs](section.rs) | One typed, dependency-aware run-knowledge section. |
| [snapshot.rs](snapshot.rs) | Canonical role-specific run-knowledge snapshots. |
| [source.rs](source.rs) | Exact content digest for one stable source identity. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ReusePremiseStatus`, `from_satisfied`, `ReusePremises`, `new`, `reuse_allowed`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [delta/](delta/AGENTS.md) | Implementation pieces for `delta.rs`: Role-specific changed-fact and current-reference packet planning. |
| [identity/](identity/AGENTS.md) | Implementation pieces for `identity.rs`: Stable caller-supplied knowledge identities. |
| [model/](model/AGENTS.md) | Implementation pieces for `model.rs`: Input-defined logical model for run-knowledge freshness and reuse. |
| [plan/](plan/AGENTS.md) | Implementation pieces for `plan.rs`: Pure fail-closed invalidation and reuse planning. |
| [snapshot/](snapshot/AGENTS.md) | Implementation pieces for `snapshot.rs`: Canonical role-specific run-knowledge snapshots. |
| [source/](source/AGENTS.md) | Implementation pieces for `source.rs`: Exact content digest for one stable source identity. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
