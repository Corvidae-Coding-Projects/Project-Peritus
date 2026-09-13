# Independent source review: knowledge planner errors

Reviewer: `/root`; implementer: `/root/sol_acceptance_completion`.

Verdict: accept this bounded increment. This is source review and an independent compiler/test rerun, not a complete obligation discharge or final-head CI result.

## Exact source and review scope

The frozen 38-file package is identified by manifest SHA256 `6854d6da0f6c0f6bd7b298f75b608a34e0937e32938ee266b233c6e00062e1d2`. All entries were checked against the frozen bytes. The eight-file patch relative to the independently reviewed 37-file admission package is SHA256 `9da7f3a1d9abadd3bece6b27ec274d55e076f7284c55edc057f56fba2cddf038`.

Reviewed all changed hunks and their executable/model/test context: delta planner and current validation; identity equality lemma; model exports, clarification predicate and prefix restriction; exact error model module; public invalidation planner; actual target validation; complete delta-packet regression test. Also reviewed the unchanged normalized-prefix freshness theorem and input-defined current-section predicate used by the new proof. Previously reviewed admission/topology/successful-output contracts remain the baseline.

## Specification and production correspondence

`clarification_targets_error` requires an actual invalid target and proves every earlier target valid. The actual validation loop selects that exact target. The full error shape includes the requested kind and section identity with source and both numeric fields absent.

`delta_planning_error` binds failure to existing production precedence: previous/current role mismatch, current/request candidate mismatch, current snapshot freshness, then prior clarification-target admission. The first two return exact plain errors. The latter errors retain complete section payloads. Both public planners retain their success iff contracts and full successful output contracts, with no added public executable preconditions.

The first stale current section is input-defined. For an actual planner prefix whose predecessors all reuse, the new lemma restricts exact correspondence to both the prior prefix and the prefix through the current index. Existing input-only normalized-plan freshness and exact-entry correspondence establish that the current executable decision matches the current input section freshness. This conditional lemma does not falsely claim every later dependency-invalidated section is itself directly stale. At the first failed decision the condition holds, giving the unique first stale section.

The identity conversion lemma is valid because `KnowledgeSectionId` contains exactly one 16-byte field; the existing closed byte view exposes that sole field to its defining module. Runtime error construction remains the original typed section error. The `?` in normalized current planning cannot propagate an unrelated target error because the same-revision request has no targets and the existing success iff contract proves that call succeeds.

All new runtime code is the actual public planner path. Explicit error branches replace equivalent `?` propagation so their precise contracts can be connected. No alternate executable implementation, assumption, external body, new runtime admission guard, scanner relaxation, or changed manifest/feature was added by this patch. The pre-existing broad missing-docs annotation in plan.rs is unchanged and is not newly justified by this review.

## Evidence checked and independently reproduced

All supplied evidence-manifest hashes were checked. The implementation's final strict proof reports 220 verified and 0 errors; default and all-feature tests each report 29 passes, and lint/format/layout/API logs report success. The supplied negative control replaces the stale-section error with a plain error and fails at the exact current-snapshot error assertion (219 verified, 1 error), followed by full 38-file restoration and a successful final proof. The reviewer inspected this negative-control evidence but did not rerun the mutation.

The reviewer independently copied the frozen package into `/tmp/peritus-parent-obligations-proof.6ib_z8mo` and ran pinned `cargo verus verify --package peritus-run-knowledge --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20`: 220 verified, 0 errors. Independent `cargo test --package peritus-run-knowledge --all-features --locked` passed all 29 tests. Logs are retained with this review. The source package has no edits after the frozen manifest.

## Limits retained

These theorems govern planner decisions and complete typed errors for supplied domain values. They do not establish digest authenticity, current observation truth, persistence, rendering, context assembly, or full caller correspondence. Those remain separate work. Existing dependency Clone-spec warnings remain visible. The review does not authorize a trust/scope reduction or satisfy final-head hosted CI by itself. Integration checks are recorded separately against the shared evolving worktree.
