# crates/orchestration/peritus-role/src — navigation

## Responsibility

Role-aware context policy for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [capability_view.rs](capability_view.rs) | Read-only operation views that can only narrow B1 role permissions. |
| [context_class.rs](context_class.rs) | Context classifications used by role visibility policy. |
| [context_policy.rs](context_policy.rs) | Immutable role-specific context policies. |
| [error.rs](error.rs) | Typed role-policy construction failures. |
| [harness_role.rs](harness_role.rs) | Explicit harness roles and their canonical B1 identities. |
| [independence.rs](independence.rs) | Projection of immutable B2 reviewer-independence requirements. |
| [lib.rs](lib.rs) | Role-aware context policy for Peritus. |
| [presentation.rs](presentation.rs) | Provider-neutral role presentation preferences. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `capability_view_is_narrow`, `reviewer_context_is_fresh`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [capability_view/](capability_view/AGENTS.md) | Implementation pieces for `capability_view.rs`: Read-only operation views that can only narrow B1 role permissions. |
| [context_policy/](context_policy/AGENTS.md) | Implementation pieces for `context_policy.rs`: Immutable role-specific context policies. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
