# crates/app/peritus-product-runner/src/execution — navigation

## Responsibility

Product-run and conversation entrypoints, live input contracts, phase orchestration, recovery, and settlement.

## Files in this folder

| File | Role |
| --- | --- |
| [acceptance.rs](acceptance.rs) | Delivery-scope-specific acceptance evidence and decisions. |
| [cancellation.rs](cancellation.rs) | Shared cancellation observation for every product-run phase. |
| [candidate_digest.rs](candidate_digest.rs) | Exact candidate identity inspection for external handoff controls. |
| [checkpoint.rs](checkpoint.rs) | Candidate observations at material tool/run boundaries; in-place axes share captured file observations, while managed Git candidates retain full repository-context fences, with an opt-in Linux same-capture identity projection and conservative fallback elsewhere. |
| [conversation.rs](conversation.rs) | Read-only conversational front end to the existing production execution pipeline. |
| [cycle.rs](cycle.rs) | One inspect-review-fix cycle of a product run. |
| [deadline.rs](deadline.rs) | Phase-aware run admission with time reserved for honest finalization. |
| [driver.rs](driver.rs) | Attempt deadline ownership, accounting, and terminal settlement. |
| [folder.rs](folder.rs) | In-place workspace capability supplied to the shared conversational/pipeline entry point. |
| [no_change.rs](no_change.rs) | Reviewed, explicitly authorized investigations which produce no patch candidate. |
| [obligations.rs](obligations.rs) | Literal public obligations carried through gates, review, acceptance, and fixer routing. |
| [resume.rs](resume.rs) | Digest-valid phase resume planning over retained product-run state. |
| [review_phase.rs](review_phase.rs) | Independent review and obligation qualification for a checked candidate. |
| [settlement.rs](settlement.rs) | Final candidate refresh and honest terminal settlement construction. |
| [modules.rs](modules.rs) | Explicit module commands and durable, separately scoped stage execution. |
| [modules/](modules/AGENTS.md) | Role tool intersection and private artifact readers. |
| [state.rs](state.rs) | Restorable product-run state and initial design/writer phase preparation. |
| [summary.rs](summary.rs) | Task-level completion summary conservation. |
| [terminal_exit.rs](terminal_exit.rs) | Active-loop exit facts consumed by the protected finalization boundary. |
| [turn_result.rs](turn_result.rs) | Completed developer-turn values shared by writer and fixer phases. |
| [types.rs](types.rs) | Public run and conversation contracts, optional durable permission revision, and internal completed-turn values. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [conversation/](conversation/AGENTS.md) | Implementation pieces for `conversation.rs`: Read-only conversational front end to the existing production execution pipeline. |
| [resume/](resume/AGENTS.md) | Implementation pieces for `resume.rs`: Digest-valid phase resume planning over retained product-run state. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
