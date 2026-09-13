# Proof-impact reconciliation checkpoint and qualification procedure

The source accounting is reconciled into an exact **pending, audit-only proposal**. GAP-02
remains open: this checkpoint does not supply a new approved PCR, final package qualification,
or protected-base authorization. Earlier approvals remain immutable.

## Historical audit source and selected base

| Input | Identity |
|---|---|
| Selected `develop` base observed on 2026-09-13 | `8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493` |
| Draft PR #74 source candidate | `d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8` |
| Candidate tree | `73791a6d6079fd026cf6595fdc37dcf565456212` |
| Unchanged protected proof-impact manifest SHA-256 | `39135a20188b9415102c51c04e44f53fad27ea73a0775bbb4623a251b88a6d44` |

The new `xtask proof-impact-inventory` audit command uses the gate's existing compilation-source
discovery, deepest package ownership, formal-class selection, manifest inventory, and shared-input
rules. It does not read the stale approval inventory as the source of expected files, and discovery
errors fail it. It neither changes the trust gate nor authorizes its own output.
The pending proposal binds the frozen Git source above. The separately retained audit-tool source
identities describe this pass's checker/exporter additions; those additions are not part of that
older commit or its historical approval. Recheck both the source freeze and protected base before
preparing a verdict.

## Final qualification preparation (2026-09-13)

The tables and retained audit artifacts below describe the earlier `d0e4f0c` source. Their original
reviewed tool sources are retained in the [prior-source archive](formal-coverage-evidence/proof-impact-reconciliation/prior-audit-source/README.md).
They are historical evidence, not final-candidate approval.

The next candidate enrolls ACTOR-0005 from the actual Crosslink agent `4jyS`, session 4, issue 75,
and ACTOR-0006 from the independently running `/root/gap2_final_review` task. The previous actor
and provenance entries are preserved; their aggregate provenance digest pointers are refreshed.
All 153 live obligations transfer current accountability to ACTOR-0005, with their `in-progress`
statuses and evidence unchanged. This transfer does not attribute historical authorship to the new
owner. Reviewer enrollment alone does not constitute a verdict.

The checker now distinguishes authorization of an unchanged base from validation of the complete
proposed candidate. Candidate validation checks its actual actors, obligations, source inventory,
trust boundaries, architecture, documentation, and workflow policy without recursively interpreting
its historical PCR as a new authorization. Canonical workflow bytes reside in the reviewed checker
so the authorization commit can compile while preserving the base's workflow files. Full
`verify-trust` continues to require the exact externally supplied review base.

After the final candidate is committed, regenerate a fresh proposal with `reconcile.py`, then run
[the serial package driver](formal-coverage-evidence/proof-impact-reconciliation/run_gates.py):

```sh
python3 docs/formal-coverage-evidence/proof-impact-reconciliation/run_gates.py \
  --candidate FULL_CANDIDATE_COMMIT \
  --plan target/FINAL_RECONCILIATION/reconciliation.json \
  --output target/proof-impact-gates/FULL_CANDIDATE_COMMIT
```

The driver requires that exact clean candidate and the complete 132-command plan, retains each
command's raw output and hash, limits Cargo to one build job and Verus to two CPUs, and stops at
any command or source-integrity failure. A repaired candidate requires a new freeze, proposal, and
complete run. Final approval requires an independent review of those exact sources and outputs.
No final package pass or PCR-0006 approval is claimed by this preparation.

The first freeze, `5cbd9efd5ef7719b1505d64350df56e1a0b58892`, was stopped after 17 passing
package commands when investigation of an older macOS CI failure found a native-controller cleanup
race. The controller could reap a child, receive its queued cleanup response, then wait on the
released child again. The repair retains that observed exit status and preserves the output-limit
error after completed teardown. Two forced-order regressions fail against the old source and pass
against the repair; the full 23-test resilience suite and strict Clippy pass. That package is class C,
so this evidence is additional to the 132 formal-package commands. The partial first run is retained
as abandoned evidence and cannot qualify the repaired candidate; all 132 commands must restart.

Live GitHub inspection found no protection on `develop`; only `main` has an applicable protection
ruleset. The selected base can be bound exactly by the local checker, but describing it as protected
does not deploy trusted authority. Live authority deployment remains GAP-07. This work does not
change repository rules or maintainer self-merge permissions.

## Exact source reconciliation

The [machine-readable proposal](formal-coverage-evidence/proof-impact-reconciliation/reconciliation.json)
includes every before/after raw-byte SHA-256, affected package set, inherited base discrepancy,
branch transition, per-obligation status, and required evidence command.

| Comparison | Changed identities | Added inputs | Removed inputs | Total transitions |
|---|---:|---:|---:|---:|
| Protected declared records to protected actual source | 176 | 42 | 0 | 218 |
| Protected actual source to candidate actual source | 196 | 209 | 1 | 406 |
| Protected declared records to candidate actual source | 368 | 251 | 1 | 620 |

An identity contains both its digest and package set. The earlier raw-digest inspection found 168
base mismatches and 361 candidate mismatches including the missing file; the larger counts above
also include package-set changes and newly discovered inputs. These comparisons overlap and must
not be added together. The protected manifest has 3,390 rows, the protected actual inventory has
3,432 inputs, and the candidate inventory has 3,640 inputs.

The removed path is `crates/app/peritus-product-runner/src/verified_api/progress.rs`. A future
approved transition must remove its applied inventory row while retaining its earlier PCR history.
The proposal records that exact removal; this pass leaves the approval manifest untouched because
no approved replacement transition exists on the protected base.

All 11 shared-input package sets expand from 65 to 66 formal packages because
`peritus-local-socket` is class H. The current set is 53 class H, 12 class V, and one class T.
The proposal uses the gate's direct-ownership/shared-input accounting; it does not claim a
transitive dependency analysis. It lists 132 required gates: one locked all-target/all-feature
ordinary test and one class-correct full Verus verification per affected package. These command
rows are requirements, not new passing results.

## Review provenance and obligation status

The five historical PCRs, all protected detached review artifacts, `actors.toml`, and
`actor-provenance.json` remain byte-identical. PCR-0005 is authentic for implementation
`a6994ce09a67b6631318d88659cd4df5ed5e81f7` and tree
`0d3c5a78fe8d8e97620378f4f365b63e90ed673b`: its 3,390 source rows, 130 gate records, four fixed
blocking findings, and 139 artifact hashes match that review. It does not approve this candidate.

The [evidence audit](formal-coverage-evidence/proof-impact-reconciliation/evidence-audit.md)
classifies the retained subsystem reviews. Of 38 `review.json` records, 36 explicitly disclaim
discharge, the stabilization record retains the incomplete full goal, and the CI snapshot's
bounded readiness does not confer final approval. These records retain their actual scope and
reviewer identity. They were not relabeled as a new ACTOR, new verdict, or final source review.

Both new Sol audits retain their actual task paths and assigned model/effort in the
[review provenance index](formal-coverage-evidence/proof-impact-reconciliation/review-provenance.json).
They are independent read-only audits of this reconciliation boundary, not PCR approval or
proof-discharge attestations. The [authorization audit](formal-coverage-evidence/proof-impact-reconciliation/authorization-audit.md)
records the exact checker constraints.

All **153 obligations remain `in-progress`**, including the 16 branch additions. No obligation was
removed, discharged, or excluded. Twenty declarations differ from the protected base, including
the four existing ordinary-evidence locator corrections. The proposal retains each declaration's
actual owner, source hashes, changed evidence paths, and absent final discharge review. Eight
referenced test files belong outside the formal impact inventory; they are separately fingerprinted
as evidence, without pretending that the current impact policy includes those packages.

## Remaining authorization work

1. Freeze the actual approval candidate with authentic owner/reviewer provenance. Recompute this
   proposal if any formal input, package class, actor record, checker source, or protected base
   changes. Retain the existing historical actors; enroll new actors only from their real execution
   records. An audit reviewer identity is not automatically a final-candidate approving reviewer.
2. Retain passing output for all 132 required package gates on that candidate. Obtain its independent
   source-bound review, explicit findings/dispositions, and closed artifact inventory. Historical
   partial checks cannot fill these rows by changing their labels or dates.
3. Append the exact reviewed PCR through the existing authorization-only phase: preserve protected
   inventory/history and protected application/shared/actor bytes, retain the exact reviewed checker,
   and add only the permitted authorization artifacts. Land that authorization first, then apply the
   exact reviewed source and inventory against the resulting protected base. The candidate commit
   must remain reachable. A new PCR on the source-changing PR cannot authorize that same PR.
4. Pass the real protected-base trust gate and separately complete trusted CI deployment and hosted
   qualification (GAP-07/GAP-08). No GitHub setting or maintainer self-merge permission changed here.

`history_is_applied` checks whether the declared history reconstructs the declared inventory; it
does not show that the protected tree actually matches those records. The inherited base drift is
therefore part of the proposed authorization, not a reason to rewrite PCR-0001 through PCR-0005 or
use the candidate's own HEAD as the protected base.

## Reproduce

Build the audit tool with one Cargo build job, then run the retained script from this worktree:

```sh
CARGO_BUILD_JOBS=1 CCACHE_DISABLE=1 cargo build --package xtask --locked
python3 docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile.py \
  --base 8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493 \
  --candidate d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8 \
  --xtask target/debug/xtask --output /tmp/peritus-impact-reproduction
```

The script materializes both immutable Git trees in disposable directories, uses the same built
exporter on each, compares emitted hashes with Git blob bytes, and refuses changed approval history
or historical provenance. Append-only candidate actor enrollment is recorded without granting authenticity or approval. It writes audit JSON only. The binary hash is build-specific; source inventory
identities are raw Git bytes. The supplied base's protection must be verified externally; ancestry
alone cannot establish that a commit is the protected base.

## Local validation

The final exporter passed 455 xtask tests (one existing ignored test), including `xtask all`,
strict all-target/all-feature Clippy, and formatting. Five disposable Git probes cover raw
export-substitution bytes, tag rejection, output/symlink preservation, failed publication, and
symlink/excessive tree rejection. Two independent regenerations produced byte-identical JSON.
The current checkout matches all 3,640 frozen impact inputs and every referenced obligation/test
file; approval artifacts and actor provenance also match the captured candidate bytes.

The [exporter review](formal-coverage-evidence/proof-impact-reconciliation/reconciler-review.md)
passed with explicit operational limits after its seven boundary findings were repaired. Both
tree inventories execute one privately frozen tool binary, and the exact audit schema is checked.
The [validation record](formal-coverage-evidence/proof-impact-reconciliation/validation.json)
retains the source hashes, command results, and raw logs. Protected `verify-trust` still fails on
the missing progress source row. No full package Verus campaign or final-candidate discharge review
was performed by this reconciliation pass, and no protected gate success is claimed.
