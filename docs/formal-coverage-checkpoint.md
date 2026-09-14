# Formal verification stabilization checkpoint

## Current campaign status

This document preserves the earlier stabilization checkpoint below. Since that checkpoint,
GAP-01 and GAP-02 have completed. GAP-02 authorization PR 75 landed before application PR 76;
the application merged into `develop` at `a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8` after
all 132 package commands and final hosted qualification passed (576 successful checks and three
expected skips). The trust check also passed against its actual authorization base. Crosslink
continuation issue 76 records the source identities, independent review and completion evidence.

GAP-03 is active on [draft PR 77](https://github.com/Corvidae-Coding-Projects/Project-Peritus/pull/77).
Its [current implementation and remaining requirements](formal-scheduler-gap3.md) supersede the
historical scheduler status below. GAP-04 through GAP-08 remain open. The draft is intentionally
unmerged; no current scheduler source is qualified by the earlier GAP-02 results.

## Historical stabilization checkpoint

This local checkpoint preserves the accumulated formal-coverage work and completes the two
patches selected on 2026-09-13. It is not a completed formal-coverage goal or a merge-ready PR.
The full campaign's production proofs, review authorization, and hosted CI remain incomplete.

## Follow-up: method ownership (2026-09-13)

GAP-01 is now closed locally. The source checker resolves methods to the unique nominal type's
module through explicit imports and re-exports, and the exact strict app-runner scope command
passes. Four ordinary method locators affected by the same defect were corrected as well.
See the [method-mapping repair and evidence](formal-method-mapping-fix.md). No obligation was
marked discharged; GAP-02 through GAP-08 remain open.

## Follow-up: proof-impact reconciliation (2026-09-13)

GAP-02 has an [exact historical pending reconciliation and final qualification procedure](formal-proof-impact-reconciliation.md): the earlier audit records 620
source identity transitions, 66 affected packages, preserved historical approvals and provenance,
and all 153 obligations still in progress. The next candidate enrolls the actual current owner and
independent reviewer; all live obligations transfer current accountability while preserving status. The proposal includes 218 inherited
protected-base discrepancies and a separate 406-transition branch comparison. The required 132
package gates, final-candidate review, and protected-base authorization remain outstanding; the
audit output does not claim their success.

## Completed in this pass

1. **Ledger extraction errors.** `RequirementLedger::extract_preimage` now specifies the exact
   first rejection and all four error fields. The contract follows collection bounds, interleaved
   identity/content validation, span, ordinal, typed shape, canonical paths, and alternative topology.
   The unchanged public `extract` wrapper propagates that error before successful hashing. Public
   regressions exercise competing failures and their payloads. Independent review passed.
2. **Scheduler dependency verification.** The existing `advance_right` proof exceeded Verus's
   default dependency budget although the package-root budget passed. A private, erased helper now
   proves the cursor facts in smaller verification steps. Runtime operations, admission conditions,
   and result contracts are unchanged. Fresh strict verification at the lower budget and independent
   source review passed. No solver limit was increased and no trust escape was added.

The [portable eight-path patch](formal-coverage-evidence/stabilization-checkpoint/two-patches.patch)
separates these two repairs from the accumulated campaign. The [source identities and review
record](formal-coverage-evidence/stabilization-checkpoint/review.json) retain exact before/after
hashes, independent reviews, commands, and raw validation output.

## Combined validation at the stabilization checkpoint

All results below are local Linux results for the integrated working sources. One Cargo build job
and at most two Verus execution CPUs were used. These results are not final-commit hosted evidence.

| Check | Result |
|---|---|
| Fresh strict scheduler proof at resource limit 10 | 375 verified, zero errors |
| Actual app-runner CI invocation: scheduler dependency | 375 verified, zero errors |
| Same invocation: ledger dependency | 361 verified, zero errors |
| Same invocation: complete selected product-runner root | 34 verified, zero errors |
| Subsequent compiler-scope evidence gate | **Failed** on OBL-0222 method-path correspondence; see GAP-01 |
| Ledger and scheduler tests, each of default/no-default/all-feature profiles | 61 passed, zero failures per profile |
| Complete product-runner library suite | 284 passed, zero failures, three existing ignored tests |
| Daemon scheduler ownership, command replay, and durable restart | Four selected tests passed |
| Strict all-target/all-feature Clippy for both packages and product runner | Passed |
| Workspace formatting | Passed |
| Complete xtask library suite | 433 passed, zero failures, one existing ignored test |
| `xtask all`, source layout, and static formal inventory | Passed |
| Protected `verify-trust` | **Failed** because proof-impact still names the removed verification-only `ProductRunProgress` source; see GAP-02 |

The static inventory was regenerated after integration and includes 153 obligations across 66
formal packages. All obligations remain in progress. `xtask all` performs local checks and does not
replace protected-base `verify-trust` authorization. Existing dependency Clone warnings are retained
in raw Verus output; their unproved semantics are not silently treated as verified.
All 4,624 files in the compiler's captured input snapshot were checked again after the run and
matched. The snapshot correctly records that these sources were not yet committed at execution.

## Explicit remaining gaps

These items preserve the original goal. They are not new implementation tasks authorized by this
checkpoint, proof exclusions, or a claim that the unreviewed remainder is infeasible.

| ID | Remaining requirement | Evidence needed to close it |
|---|---|---|
| GAP-01 | **Closed locally by the [method-mapping repair](formal-method-mapping-fix.md).** OBL-0222 and OBL-0224 now name `accounting::AccountingState::{apply_usage,apply_work}` while retaining their implementation files. | Exact owner resolution, wrong-owner/decoy regressions, independent source review, and the strict app-runner scope gate pass. Full protected authorization and hosted qualification remain GAP-02/GAP-07/GAP-08. |
| GAP-02 | [Source accounting and provenance audit prepared; authorization remains open](formal-proof-impact-reconciliation.md). The exact pending proposal retains earlier approved transitions and all obligation statuses. | All 132 final-candidate package gates, authentic final source review, separate protected authorization and application, and actual trust-gate success against the correct base. Inventory output alone does not discharge obligations. |
| GAP-03 | Complete scheduler command admission, cancellation/tree and worker-loss loops, whole reducer/replay/terminal composition, and termination. The cancellation draft remains unimplemented. | Contracts on production-called code, sufficient independent specifications, strict proofs, regression/caller tests, and independent review for each listed relationship. |
| GAP-04 | Complete context selection, compaction, working-state and rendering/resume correspondence beyond the reviewed graph/reuse kernels. | Exact input/output/state relationships on the real implementations and consumers, with proofs and reviewed boundaries. |
| GAP-05 | Finish remaining kernel/agent/protocol lifecycle composition, journal persistence/recovery, product ordering/cancellation, and end-to-end release/security qualification. | A current obligation-to-specification-to-executable-to-caller map identifying what later increments already closed; complete the remaining feasible relationships and review their exact sources. Historical audit paragraphs are not a substitute for that final map. |
| GAP-06 | Justify residual external and unsupported boundaries: observation/provenance truth, cryptographic execution, clocks, persistence, effects, and any code outside the supported verifier subset. | Precise scope, demonstrated technical limitation, assumptions, compensating tests, accountable owner, and independent review. Empty trust/exclusion registers do not establish absence of boundaries. |
| GAP-07 | Deploy and exercise trusted CI that cannot authorize its own weaker checker or proof scope, including a legitimate bootstrap/update path. | Installed trusted validation, source-bound result authority, negative and legitimate end-to-end cases, and exact candidate/base binding. The repository workflow alone does not establish deployment or exclusive authority. Preserve maintainer self-merge. |
| GAP-08 | Deliver the final reviewed change to `develop` and complete full qualification. | A final PR, reviewed records matching its exact commit, and every required hosted workflow passing, including platform checks. This local checkpoint does not supply those results. |

The [campaign audit](formal-coverage-audit.md) and retained subsystem reviews contain the detailed
history. Later reviewed ledger, qualification, role, knowledge, and release increments supersede
their earlier gap descriptions; do not reopen them merely because an older paragraph says open.

## Review and resumption

The checkpoint commit includes the formal source changes, workflow/checker drafts, and retained
evidence. Machine/provider integration files under `.claude`, `.codex`, `.crosslink`, plus `.mcp.json`
and the locally generated `AGENTS.md`, are excluded. No live GitHub rule, reviewer requirement,
deployment, or provider setting was changed in this pass.

Raw evidence retains its reviewed bytes, including trailing log blank lines and the context-space
lines required by unified patches. A full staged whitespace scan flags those data artifacts;
the source/documentation whitespace check excludes the raw evidence directory and passes.
The pinned `.gitattributes` policy is unchanged.

The next delivery blocker is GAP-02: final source/review reconciliation. Broader proof
work remains explicitly listed above. This pass stops at the local checkpoint; it does not publish
or merge a PR or declare the original goal complete.
