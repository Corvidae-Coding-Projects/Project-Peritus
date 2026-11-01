# Stage 2: durable read-only inspection plans

Baseline: `d106c9b13318fbcdd3d816ed63ae48532806fa7e`. Discovery-only owner module: `local_context::operation_plan::praxis_plans`. This document records contracts and the implemented search boundary; execution results belong to the coordinating runner.

## Actual path exercised

The fixture calls real `LocalMemory::load_scoped`, backed by the ordinary SQLite journal and artifact store, then `begin`, `observe_message`, `prepare_view`, and `publish`. A cfg(test)-only `LocalContextHandle::praxis_from_memory` constructor wraps that actual owner without changing production visibility. Plan proposals are archived as ordinary assistant tool-call messages. The campaign calls the complete private `operation_plan::execute`, including its whole-plan validation, input/scope checks, committed reader guard, per-child admission, sequential ordinary executor dispatch, exact artifact storage, host receipts, response paging, cursor reconstruction, and current-permission exact-result reader. Parent observations are settled with the real `observe_tool` path.

The ordinary `DeveloperToolExecutor` is a recording, scripted dependency. Its schema admission uses the same ordinary argument API as callers, but its supplied response values are explicit fixture data. It records exact calls and refuses execution without a fresh host admission and the preceding completion. `DeveloperInteraction` receives the actual admit/complete/activity callbacks. The campaign changes run cancellation, provider cancellation, input revision, read permission, and callback control decisions at deterministic child boundaries. It does not use a live provider or start processes.

This tests the real plan coordinator and durable host integration. The native implementations of workspace read/list/search and daemon authority persistence remain separate campaigns; the scripted dependency is not claimed as native file-tool qualification.

## Independent interpreter and observations

`NodeSpec` describes a sequence of literal and earlier-output-dependent read/list/search calls. A tiny independent interpreter computes arguments, expected calls, supplied dependency outputs, error disposition, and the stopping prefix. Its JSON Pointer decoder traverses explicit objects/arrays and decodes `~0`/`~1` itself; it does not call production plan validation/resolution or `Value::pointer`.

Result-reference cases cover direct fields, array indices, slash/tilde-containing keys, missing fields, and schema-incompatible referred values. Shrinking can remove nodes: logical source selectors map into the remaining earlier-node range, preserving valid relationships without skipped work. Invalid-graph families separately violate exactly identified constraints.

For each successful child, the oracle checks exact tool/name/arguments and admission/completion sequence and invocation binding, public start/finish order, host child accounting, receipt order, error classification, result byte length/digest, optional inline JSON, and final page state. A second ordinary sequential executor is driven with the independently resolved calls; its observations must agree with the plan's retained results. This establishes plan-versus-sequential equivalence for the declared dependency contract.

Every returned exact result is read through `inspection_result_read` with varied byte pages including multibyte boundaries. Concatenated bytes must equal the independent original canonical JSON, and each page's offset/length/digest must agree. Plan response byte bounds and continuation progress are checked independently. Reopen cases drop the last owner and reconstruct from the real journal/checkpoint/artifacts before continuing. Disabling execution after a version-4 plan still permits exact reads of historical results.

## Generated and forced histories

- Execution enabled/disabled, mixed read/list/search, literals and previous-result references, 1–8 nodes, page budgets 2048–4096, Unicode output sizes, and reopen between response pages.
- Returned child errors and structural executor errors; cancellation of run/provider; newer input; read revocation; pre-child admission yield; post-child yield/stop. Every case compares the independently expected dispatch prefix and permits no extra child.
- Whole-plan rejection before any child: duplicate IDs, forward or undeclared dependencies, literal/null/empty conflicts with references, invalid pointer escapes, forbidden mutation tool, unknown argument, empty/65-node graph, ambiguous request, wrong scope/revision, and absent durable provider proposal.
- Malformed, foreign-scope, and proposal-instead-of-continuation cursors, including reconstruction before misuse. Invalid cursors must dispatch zero additional children.
- A manually injected malformed *archived descriptor* refers to a nonexistent digest or a length above SQLite's representation ceiling. Reads must reject it. The actual previously executed result remains readable. These are corruption fixtures, not production-generated records.

Metrics report enabled/disabled plans, actual ordinary-equivalent calls, fresh admissions, result pages/reconstructed exact results, response pages/reopens, stop families, rejected graph/cursor classes, rejected archive classes, denied reads after permission revocation, and disabled-execution historical reads. Case count alone does not establish that these mechanisms ran.

## Calibration

`calibration/stage2-plans.patch` changes the current Read-capability check in `Admission::current` to inspect Write instead. The witness runs a three-read plan, revokes Read after its first child, and retains Write. The unmodified coordinator yields immediately after that committed result. The mutant attempts another child; the ordinary executor rejects revoked read authority and the coordinator commits an unexpected additional error receipt, which the independent prefix/receipt oracle detects. This preserves ordinary executor checks and calibrates sensitivity to a real coordinator admission defect rather than replacing dependency behavior.

Apply only in the disposable calibration worktree, retain baseline/mutant/restored evidence, and restore before normal search. No mutation has been applied by this worker.

## Source and caller coverage

Fully read: all `local_context/operation_plan/{mod,contract,admission,execution,validation,state,read,records}.rs`, source-policy traversal, ordinary deferred argument validation, and developer interaction/executor contracts. Supporting inspection followed the actual local-memory owner/storage constructors, ingestion/pending projections, recovery/checkpoint/dependency-guard paths, `LocalContextHandle` opening and context-port operations, `MemoryTools` plan dispatch, schema catalog, and provider-profile/canonical-call/message constructors. Those supporting files were selectively inspected; this is not a claim of complete source review of their owning subsystems. Parent/navigation documents were read before implementation. The actual caller path is `MemoryTools::execute_inner` → `operation_plan::execute`; result receipts return to source admission and recovery.

## Explicit gaps

The campaign exercises normal SQLite reopen and targeted archived-descriptor rejection. It does not simulate power loss, fsync/rename failure schedules, SQLite corruption, real daemon permission storage races, provider inference, native workspace-tool behavior, concurrent calls, or arbitrary 64-node production-scale plans. Source-policy path revocation beyond the global Read capability and repeated reuse of an already-consumed old cursor need additional specified contracts. Parent process accounting/decision/read-reuse behavior belongs to the sibling Stage 2 campaigns.

No build, generated search, replay, or calibration result is claimed here until actual coordinator evidence is recorded.
