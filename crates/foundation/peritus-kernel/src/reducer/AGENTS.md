# crates/foundation/peritus-kernel/src/reducer — navigation

## Responsibility

Implementation pieces for `reducer.rs`: Total authoritative lifecycle reducer.

## Files in this folder

| File | Role |
| --- | --- |
| [acceptance.rs](acceptance.rs) | Exact-current-evidence acceptance transitions. |
| [action.rs](action.rs) | Action proposal, authorization, dispatch, and completion transitions. |
| [attempt.rs](attempt.rs) | Attempt admission and candidate-submission transitions. |
| [refinement.rs](refinement.rs) | Executable audit of the safety-critical reducer refinement boundary. |
| [review.rs](review.rs) | Review-cycle transitions. |
| [run.rs](run.rs) | Run admission, control, and terminal transitions. |
| [session.rs](session.rs) | Session transitions. |
| [turn.rs](turn.rs) | Turn transitions. |
| [waiver.rs](waiver.rs) | Finding-waiver transitions bound to current B2 evidence. |
| [waiver_evidence.rs](waiver_evidence.rs) | Exact evidence searches used by finding-waiver transitions. |

Owning module: [reducer.rs](../reducer.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
