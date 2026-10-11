# Production formal verification work record

This is an active audit and implementation record, not a proof discharge or final approval.
The complete goal is to audit and complete all feasible production proof coverage, reconcile
reviewed evidence, and enforce its preservation in CI. A passing helper proof, package opt-in,
API parity test, or source fingerprint does not establish an end-to-end behavioral guarantee.

The current [stabilization checkpoint](formal-coverage-checkpoint.md) records the completed ledger
error and scheduler dependency-budget patches, combined local validation, and the explicit remaining
gap list. It also records the failed compiler-symbol scope gate after successful proof compilation.
That checkpoint supersedes earlier statements below that these two patches are pending.

## Baseline and custody

- Audit base: `1aa9282ff2cfcbf8fac325f11f157f408d33ee30`, 2026-09-12.
- Work branch: `feature/formal-coverage`, based on both PR #73 provider/runner repairs.
- Current branch base: accepted `develop` merge `8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493`. Its committed source tree equals the audit base; the fast-forward preserved the complete working diff and status, as recorded in [develop synchronization evidence](formal-coverage-evidence/develop-sync.json).
- Initial independent source audits: `/root/formal_kernel_audit` and
  `/root/formal_runtime_audit` in this Codex thread. These audits are not final review approval.
- No existing obligation has been marked discharged and no historical proof-impact record has
  been rewritten by this work.

At the audit base, 66 packages opted into Cargo-Verus: 12 V, 53 H, and one T. The obligation
register contained 137 in-progress entries, of which 103 cited Verus evidence and 34 cited only
tests. Four V packages had no owning-crate obligation entries: `peritus-types`,
`peritus-obligations`, `peritus-run-knowledge`, and `peritus-run-settlement`.

The explicit `verify-trust` command failed with 221 diagnostics: 168 changed source fingerprints,
42 missing fingerprints, and 11 changed affected-package sets. `xtask all` deliberately omitted
protected proof-impact authorization. These are evidence/governance failures, not a count of
failed SMT obligations.

Live GitHub ruleset 21198414 targeted only `main`, required GitHub Actions status contexts, and
required zero approving GitHub reviews. It did not cover `develop`. The candidate workflow
compiled and executed the candidate's own `xtask all`. The current documentation explicitly
defers immutable independent workflow authority. Final enforcement must close these gaps;
repository-controlled assertions about approval are insufficient.

## Initial coverage classification

The H-package classification below describes source selection and observed production callers.
It does not certify that every selected item has a sufficient specification or verified caller.
All listed names have the `peritus-` prefix.

| Category | Packages |
|---|---|
| No source verification markers (13) | product-runner, mcp, plugin-host, plugin-sdk, codec, provider-core, provider-anthropic, provider-compatible, provider-google, provider-openai, protocol, tools-quality, tools-shell |
| Verification spread through implementation modules (4) | approval, leases, context, memory |
| Ordinary/verified translation boundaries (3) | app-protocol, daemon, product-state |
| Selected predicates without observed production callers (7) | agent, debugger, eval, evolution, telemetry, tool-protocol, trace |
| Separate mathematical lifecycle/replay models and ordinary checks (5) | gates, review, harness, orchestrator, scheduler |
| At least one production call to a selected executable helper (21) | artifact-store, collaboration, evidence, git, journal, migrations, model-protocol, local-socket, network, patch, process, projection, sandbox, sandbox-linux, sandbox-macos, sandbox-windows, secrets, tool-router, tools-fs, tools-git, workspace |

Pinned Verus selects annotated items; package `verify = true` is not function coverage. The
product runner swaps production execution for `verified_api` during Verus builds. That API
contains error-returning effect stand-ins and an uninhabited command runtime, and its source has
no verification markers. Type parity establishes an interface shape, not equivalent behavior.

## Required implementation work

1. **Preserve existing substantive refinements.** Budget command/input/result relations, policy
   evaluation and capability-use contracts, gate ordering certificates, and checked primitive
   constructors provide meaningful guarantees. Their callers and specifications must survive
   the changes below.
2. **Complete acceptance semantics.** Define independent predicates for required gates,
   artifacts, review quorum/categories/independence, waivers, approvals, and policy limits;
   refine the real collection traversals and propagate their guarantees into acceptance.
3. **Complete kernel semantics.** Include actual commands and inputs in reduction contracts;
   prove parent/identity validity, uniqueness, exact frames, authorization witnesses, and
   acceptance evidence. The baseline relation's permissive cases are not complete command
   semantics. INV-020 and OBL-0117 currently have test-only evidence.
4. **Complete release/security readiness.** Prove raw Ready verdicts against exact candidate,
   evidence, and time inputs. Output predicates that already include the desired flags do not
   establish the input conditions used by production qualification consumers.
5. **Prove obligation qualification, settlement, and knowledge planning.** Cover actual ledger
   traversal, alternative completion, evidence matching, checkpoint monotonicity, invalidation,
   reuse, cancellation, rejection preservation, and accounting. Add missing registry entries.
6. **Verify production runtime kernels.** Connect the real protocol arithmetic, journal
   idempotency decisions, agent reducer, scheduler reservations, product accounting, termination,
   and recovery decisions to exact contracts. Prove ordinary-to-verified enum mappings rather
   than treating conversions as self-evident.
7. **Audit every remaining boundary.** Record a precise unsupported operation, source scope,
   observed compiler limitation, assumptions, compensating evidence, owner, and independent
   reviewer. Pinned Verus has async and VecDeque examples/specifications; neither feature is a
   demonstrated blanket exclusion.

## First implementation increments

- Seven protocol delivery, terminal, and artifact predicates now have exact postconditions on
  their executable bodies. Six have production callers; `output_position_is_valid` is unused.
  Strict package verification reported 86 verified and zero errors; 89 ordinary tests and strict
  Clippy passed. An isolated copy with the completion equality reversed failed its postcondition.
  Zero-length artifact arithmetic remains valid; positive chunk admission is a separate caller
  condition. The subscription/artifact/terminal state mutations remain separate proof work.
- Acceptance-contract binding and role-capability construction/query now preserve their exact
  supplied identities, role, and operation membership. Contract evidence, gate graph, and
  completion-limit accessors have exact specification views. Completion-policy construction
  proves successful admission precisely when both supplied limits are nonzero.
- Required-artifact evaluation is equivalent to independently defined current declaration and
  required-presence predicates. Required-gate evaluation proves current declaration, first
  current passing result for each declared gate, and passing-attempt bounds. Acceptance implies
  these predicates and binds both completion limits to the supplied immutable contract. Strict
  Verus reported quality-policy 200 and kernel 274 verified, zero errors; 42 focused ordinary
  tests and strict Clippy passed. Canonical evidence-constructor semantics, review requirements,
  and waiver/approval authorization remain open, as does authentication of observations.
- Journal command classification now proves exact New/Replay/Conflict results against every
  digest byte. The SQLite append path calls that implementation. Its strict pass reported 16
  verified and zero errors. SQL transaction, hashing, and crash-recovery proofs remain separate.
- A reproduced developer-usage defect partially changed counters before returning a late
  cost-overflow error. The repair shares the real numeric state and mutable update between
  ordinary and Verus builds. It proves exact overflow admission, every updated counter, and
  complete unchanged state on rejection. Strict agent verification reported 17 verified and
  zero errors. The two-package runtime suite passed 125 tests and strict Clippy. Independent
  review by `/root/formal_kernel_audit` accepted this bounded increment and independently reran
  six tests. Provider-counter normalization and broader product accounting remain open.

Each increment requires strict package verification, relevant runtime regression tests,
formatting, Clippy, and later independent review of the final source identities. These increments
do not complete the larger state-transition or caller proofs. Verification totals above describe
the pinned compiler's package output; they must not be summed as unique business guarantees.

An initial checkpoint of all 65 strict V/H package checks passed after these increments, across
the six existing proof shards. The separate non-strict trust-aware T-package pass reported six
verified and zero errors.
The first capture mechanism needed correction after independent review: imported symbols in a
combined report could mask a missing package report. The revised mechanism uses a fresh
invocation per package, rejects foreign-root queries, and retains genuine zero-query reports
without calling them proofs. Independent review accepted the revised collector as a bounded
reporting increment; all 65 packages then passed the revised full shard rerun. Its reports contain
121 distinct registered evidence symbols and 4,253 distinct solver-query functions: 3,662 exec,
435 proof, and 156 spec. These counts describe compiler work, not business guarantees or a
percentage of production correctness.

At that checkpoint, thirteen packages reported zero solver-query functions, including the product
runner. They are the 13 packages listed in the initial no-marker category above. This confirms
the baseline correspondence gap; successful Cargo-Verus compilation does not establish that
those production execution paths have been proved.

## Subsequent reviewed increments

The product runner now calls a shared, unconditional numeric accounting kernel. Its request,
retry, tool, compaction, and response-replacement operations have exact arithmetic admission and
update contracts, including complete state preservation on arithmetic rejection. This fixed a
reproduced retry-overflow failure that incremented model requests and erased prior response usage
before returning an error. `ProductRunProgress` is the same type in ordinary and Verus builds;
the duplicate verification-only representation was removed. Strict Verus reported 34 verified
and zero errors. The implementation suite passed 304 tests; final focused checks passed 16 tests
and strict Clippy. Independent reviewer `/root/formal_kernel_audit` reran 16 tests and checked all
11 source/configuration hashes plus the deleted duplicate representation.

This accounting proof preserves replacement semantics when an explicit total corrects an earlier
derived total downward. Hard-ceiling rejection retains recorded work, and the proof does not claim
that counters always remain below the ceilings. Event identity/order, dispatch, persistence,
provider normalization, cancellation, clocks, and resource sampling remain separate work.

Review evaluation now has an exact predicate over declared and covered categories, current-review
count and quorum, all six configured checks on supplied independence facts, and the contract cycle
limit. Acceptable decisions imply that predicate. Upstream policy/observation getters and successful
constructors preserve the proof-relevant supplied inputs. Strict quality-policy verification
reported 204 verified and zero errors; spec/quality/kernel runtime checks passed 54 tests and strict
Clippy. Independent parent review reran 40 spec/quality tests and checked all 14 source hashes.
This establishes checks on supplied actor/provenance values; it does not authenticate them or prove
that distinct ancestry digests represent disjoint causal histories.

Canonical constructor admission is now proved for the actual `ReviewObservation::new` and
`AcceptanceEvidence::new` implementations. Their result is successful exactly when the supplied
collections satisfy the independent ordering, uniqueness, resolution-freshness, and subject
consistency predicates, and success preserves every supplied field and collection. Strict adjacent
identity ordering also proves global uniqueness. The constructors establish private type
invariants; no public executable preconditions or trust escapes were added. Independent parent
review inspected the 13-file increment, compared the old validation/error paths, and checked
all source hashes in the [retained constructor review checkpoint](formal-coverage-evidence/canonical-constructors/review.json) (source-manifest SHA-256
`ff006ad7319918285b97c29906fdddf6d0b0fb021de09eff841ffa16effc8e20`). A fresh parent strict run
reported spec 176, quality-policy 220, and kernel 274 verified, all with zero errors; the parent
also reran 50 quality-policy/kernel runtime tests. The checkpoint retains the source manifest,
exact commands, raw verifier and test output, review scope, and content hashes.
This review accepts the bounded constructor increment, not an obligation discharge. Missing
approval remains constructor-admissible, supplied reviewer provenance is not authenticated,
and complete error-result correspondence is tested rather than formally specified.
The later waiver-authority checkpoint below supersedes the constructor checkpoint's hashes for
the two observation files containing new ghost view bridges; the earlier checkpoint remains
historical evidence of its exact reviewed source.

Waiver evaluation now proves exact current-observation lookup from constructor-derived identity
uniqueness, declared authority/evidence/approval checks, and the failure reason's precedence.
The actual waiver phase returns true exactly when all supplied current waivers are valid and
all current blocking findings are resolved or authorized for waiver. Every invalid supplied
current waiver is also proved to remain diagnosed by its complete finding identity through the
subsequent acceptance phases. Parent review read all new helpers, predicates, view bridges,
integration and scenario tests; the independent test rerun passed 52 tests. Final strict
verification reported quality-policy 232 and kernel 274 verified, zero errors; strict Clippy
passed. The [retained waiver review checkpoint](formal-coverage-evidence/waiver-authority/review.json)
binds fourteen increment files and retains implementation and independent test evidence.

This establishes checks on supplied observations, not external authority or evidence
authenticity. The exact waiver failure reason is proved separately from final diagnostic
coverage; complete diagnostic ordering, multiplicity and absence of false positives are not
claimed as a theorem. The later acceptance-equivalence checkpoint below completes final and
unexpected approval semantics and top-level acceptance equivalence; the subsequent kernel
waiver checkpoint connects successful grants to their stored request and supplied authority.

The actual acceptance evaluator now decides the exported input-defined predicate exactly:
acceptable status and complete phase checks each equal `input_defined_acceptance`, and complete
inputs imply empty diagnostics. The predicate covers the contract's acceptance-spec identity,
every observation's revision, gates, artifacts, reviews, blockers/waivers, and final/unexpected
approvals. Final approval has an exact missing-authority-denial diagnostic priority and exact
append behavior. The no-growth proof for every successful phase closes the previous sufficiency
gap. Independent parent review accepted the fourteen-file increment and reran the combined
quality-policy/kernel suite: 40 plus 14 tests passed. Strict quality-policy verification passed
235 verified, zero errors; Clippy and package formatting passed. The
[retained acceptance-equivalence review](formal-coverage-evidence/acceptance-equivalence/review.json)
records hashes, commands and raw evidence. This is a predicate over supplied typed observations;
it does not authenticate their provenance. Stored contract digest/binding checks are separate
kernel preflight behavior.

The kernel now carries caller contract/evidence bindings through `ReducerInputs` and the actual
acceptance helper. On a successful evaluation, the emitted event is `AcceptanceAccepted` exactly
when those inputs satisfy the policy predicate; otherwise it is `AcceptanceNeedsChanges`.
Subject identity, revision preservation and unchanged state on helper errors are also proved.
The public reducer propagates the conditional guarantee for an `EvaluateAcceptance` command
that emits `AcceptanceAccepted`. Independent Sol review accepted the three-file parent increment;
strict kernel verification passed 274 verified, zero errors, the combined 54 tests passed, and
standard Clippy/format/diff checks passed. The
[retained kernel acceptance review](formal-coverage-evidence/kernel-acceptance-binding/review.json)
binds that exact source. This theorem does not yet prove projection matching, full lifecycle
admission/rejection equivalence, or that every other command family cannot emit acceptance.

The [kernel waiver checkpoint](formal-coverage-evidence/kernel-waiver-binding/audit.md) closes
the grant's exact stored review/run binding and original-input authority connection. A baseline
regression reproduced approval using a finding from another review cycle; the repaired helper
rejects it and preserves the Requested waiver. Explicit semantic Clone contracts carry the
original aggregate's revision, reviews, waivers, and other fields into the working copy.
Parent reviewed all 22 source hashes, then reproduced 295 strict verified checks with zero
errors and 55 ordinary quality-policy/kernel tests. The kernel root has no remaining Clone
warnings. The [retained review](formal-coverage-evidence/kernel-waiver-binding/review.json)
states the conditional public guarantee and wider lifecycle/authority limits. Consumer Copy
substitutions and their broader lint/test evidence are reviewed separately.

Agent pause, resume, cancellation request, and cancellation finish now call shared verified
control functions operating directly on the production phase and saved-phase fields. Their
contracts specify exact admission, error precedence, success updates, and unchanged fields on
rejection; the same phase enum and const tags are used in both builds. Both reduction and replay
call these functions. Parent source review passed at the seven-file manifest
`/tmp/peritus-parent-agent-control-review-hashes.txt`, with five public integration tests and an
independent strict proof importing the actual phase/control files (10 verified, zero errors).
The final ordinary agent suites passed 78 default-feature and 70 bridge-disabled tests. After
the concurrent authority increment froze, final package verification passed with 27 verified and
zero errors, and both standard dependency-inclusive Clippy configurations passed. All seven
reviewed source hashes still matched. The [retained control review checkpoint](formal-coverage-evidence/agent-control/review.json)
contains the final source manifest, exact commands, raw package evidence and independent
proof/test evidence. This accepts the bounded control increment, not a whole-reducer proof or
an obligation discharge.
Recovery completion remains a supplied fact, and the cancellation-phase proof does not establish
external effect termination. A regression explicitly retains an outstanding dispatched tool when
the pure cancellation phase completes.

The subsequent [phase checkpoint](formal-coverage-evidence/agent-phase/audit.md) makes reduction
and replay call an exact verified phase kernel over the actual command vocabulary. The legacy
tag relation now includes retry and is explicitly only an existential projection. A baseline
regression reproduced replay accepting a Failed event after termination; the shared kernel
rejects late Failed and Exhausted commands. Exact semantic Clone implementations preserve the
newly verified command records without warning suppression. Strict verification passed 56 checks
with zero errors and no warnings, all-feature/no-default-feature tests passed 84/76 respectively,
and both strict Clippy configurations passed. Parent reviewed the 29-file source checkpoint and
the [retained evidence](formal-coverage-evidence/agent-phase/review.json). Full payload, digest,
effect, and durable replay guarantees remain separate work.

The initial all-package counts above predate these increments. A complete final source-bound run
and final obligation/source-record reconciliation are still required.

The rerunnable audit command is `cargo run --locked --package xtask -- formal-inventory`.
Its [generated JSON inventory](formal-coverage-inventory.json) includes every declared obligation,
trust entry, exclusion, and formal package, with available local compiler-scope observations.
Current byte hashes cover register files, referenced source/evidence files, and formal manifests.
The output explicitly does not assert observation freshness, complete compilation-input identity,
production correspondence, or independent review. Regenerate it after source or evidence changes.

### Budget production correspondence

The [budget correspondence checkpoint](formal-coverage-evidence/budget-correspondence/audit.md)
maps INV-012 and OBL-0104 through OBL-0106 through the public reducer, its verified
runtime validators, and actual product/journal consumers. A fresh strict run passed
447 verified budget checks and the ordinary suite passed 27 tests. No budget code changed.
The mapping distinguishes the unfolding lemma from the executable proof composition
and records the separate overrun-fault accounting semantics. The
[independent bounded review](formal-coverage-evidence/budget-correspondence/review.json)
accepted this mapping and verified its 142 source hashes. It leaves OBL-0105's overrun
wording and OBL-0106's explicit whole-ancestor correspondence open at that checkpoint.
It does not discharge the entries or justify the external boundaries.

The subsequent [ancestor theorem](formal-coverage-evidence/budget-ancestry/audit.md)
strengthens the actual public transition result: every changed account has equal accounted
consumption deltas in all five dimensions at every position on any finite actual pre-ledger
parent path. Induction composes the checked immediate-parent relation with account identity
uniqueness; runtime code and public execution preconditions are unchanged. Independent Sol
review accepted the exact three-file increment. Isolated and integrated strict runs both passed
449 checks with zero errors; 27 ordinary tests, strict Clippy, and formatting passed. The
[retained review](formal-coverage-evidence/budget-ancestry/review.json) records those results and
limits. New allocation children, construction of a root path for every account, external usage,
and durable commit correspondence are outside this theorem. OBL-0105's separate overrun case
still needs precise register wording, and no whole obligation is discharged here.

### Release and security readiness follow-up

The [readiness admission checkpoint](formal-coverage-evidence/readiness-admission/audit.md)
proves that an actual release Ready result has contributors for all 44 evidence requirements,
bound to every candidate identity component and the current observation window. Both policy
decision constructors have exact assessment/result contracts. H0 production qualification now
calls a verified reduction of its actual 42 probe outcomes; H4's operator calls the verified
eight-check admission path. Parent review checked all 33 increment source hashes, the actual
caller/validator mappings and preserved failure paths, then independently reproduced 82 tests
and strict proof results of release-policy 304 and security-policy 164, all with zero errors.
The [retained review](formal-coverage-evidence/readiness-admission/review.json) binds this source
checkpoint and its explicit limits.

The finite reductions do not prove the ordinary validators or outcome classification. Complete
input semantics for release qualifications/reviews/findings and security-policy traversals remain
further work. Supplied signatures, external observations, and reviewer identity are not
authenticated by these proofs. No full readiness obligation is discharged, and ordinary-Rust
package classification is not accepted as a technical reason to exclude feasible proof work.

The later [security controls checkpoint](formal-coverage-evidence/security-controls/audit.md)
establishes exact first-observation admission for every requirement and criterion, including
candidate identity, Passed outcome, and nonzero evidence digest. Parent reproduced 174 strict
verified checks and all seven ordinary tests from the frozen source. The
[bounded review](formal-coverage-evidence/security-controls/review.json) retains the exact source
and successful evidence. The later [full-input checkpoint](formal-coverage-evidence/security-full-input/audit.md)
has now been independently reviewed and reverified: the raw stored H0 Ready verdict is equivalent
to the complete seven-phase predicate over supplied input data. All 19 source hashes matched;
fresh strict Verus passed 192 checks and all seven package tests passed. The
[full-input review](formal-coverage-evidence/security-full-input/review.json) records exact scope
and remaining constructor, external authenticity, and end-to-end qualification boundaries.

The [release input-fold checkpoint](formal-coverage-evidence/release-input-folds/audit.md)
connects the actual qualification, review, finding and waiver traversals to exact supplied-input
reductions and the raw stored H4 verdict. Parent independently reviewed all 31 sources and
reproduced 327 strict checks and 29 tests. Its [review](formal-coverage-evidence/release-input-folds/review.json)
keeps full declarative all-input readiness, constructor admission, criterion/artifact equivalence,
and external authenticity open; these reductions alone do not discharge the complete H4 policy.

The [input-to-verdict followup](formal-coverage-evidence/release-input-readiness/audit.md)
now connects all artifact and criterion reductions and diagnostic emptiness to the raw Ready
verdict and is_ready. Parent independently reviewed twenty changed files and reproduced 336 strict
checks and all 29 tests. The [bounded review](formal-coverage-evidence/release-input-readiness/review.json)
distinguishes exact finite operational folds from the still-open quantified policy characterization,
individual output-slot correspondence, exact counts/digests/diagnostic content and remaining
constructor admission equivalences. Supplied observation authenticity remains outside the reducer.

The delegated audit also suggested making H4 a prerequisite for the release workflow's staging
job. Parent inspection did not establish that as a publication bug: `release-stage` validates an
exact **unpublished draft**, uploads its installers, and validates that it remains a draft.
The workflow and its existing regression test explicitly preserve that boundary. H4 has a
separate documented evidence-finalization command. No additional release job or publication
restriction is introduced on the basis of the missing H4 invocation alone.

### Candidate settlement domain

The [settlement checkpoint](formal-coverage-evidence/settlement-domain/audit.md) proves exact
identity and evidence binding, stage support, successor admission, and the actual reducer's
exactly-once terminal decision. Rejections preserve the complete state. Waiting, cancellation,
and recovery override candidate qualification; other causes retain the existing disposition
rules. An independent Sol review accepted all nine source files and actual production callers.
Isolated and integrated strict verification passed 71 checks with zero errors; all nine tests
passed in both sources, and isolated strict Clippy/formatting passed. The
[retained review](formal-coverage-evidence/settlement-domain/review.json) records the two existing
generic Clone warnings and limits: supplied evidence truth, external effects, persistence,
clocks, and complete recovery are not proved by this domain increment.

### Obligation evidence and performance qualification

The [obligation checkpoint](formal-coverage-evidence/obligations-binding-performance/audit.md)
fixes a reproduced performance qualification false positive: saturating subtraction erased a
900-unit slowdown under a ten-unit noise margin. Exact threshold mathematics now reaches the
production satisfies method. The same increment proves canonical evidence-path admission,
full-identity currentness and membership, semantic clones, and failure-owner disposition.
Independent source review accepted all 11 files; isolated and integrated strict verification
passed 192 checks and all 16 package tests. Reintroducing only the old arithmetic fails the new
postcondition. The [retained review](formal-coverage-evidence/obligations-binding-performance/review.json)
keeps complete qualification traversal, other typed evidence, observation truth and remaining
semantic Clone contracts explicitly open.

The [typed evidence followup](formal-coverage-evidence/obligations-typed-evidence/audit.md)
adds exact stored-field and satisfaction contracts for browser, lifecycle, direct and external-effect
evidence, complete semantic clones, and exact variant binding selection. The actual external-effect
qualification branch calls the verified reducer. Independent review accepted five exact source files;
isolated and integrated strict verification passed 193 checks and all 16 tests. Nine obligation
Clone warnings remain. [Retained evidence](formal-coverage-evidence/obligations-typed-evidence/review.json)
keeps schema coverage, whole qualification traversal and supplied observation truth explicitly open.

The [directional schema followup](formal-coverage-evidence/obligations-schema/audit.md) proves
exact constructor admission and successful field retention, full-byte canonical uniqueness,
and coverage iff the stored direction matches and every required field identity is observed.
Actual schema clones and all six evidence-variant clones preserve complete semantic content.
Independent review accepted all eight source files and reproduced 222 strict verification results
with zero errors and all 21 tests passing. The
[retained review](formal-coverage-evidence/obligations-schema/review.json) keeps exact error-value
contracts, full qualification traversal, remaining ledger/provenance contracts and observation
truth open. Five obligation Clone warnings and two dependency Clone warnings remain.

The [foundation followup](formal-coverage-evidence/obligations-foundations/audit.md) closes those
five obligation Clone gaps and proves exact errors, limits, conditions, source/clause field
retention, path roles, canonical path validation and requirement-entry admission. Independent
source review accepted nine files. Isolated and integrated strict verification each passed 225
results with zero errors, and all 25 tests passed. A role-only clone mutation fails its new
complete-content postcondition. The [review record](formal-coverage-evidence/obligations-foundations/review.json)
keeps complete ledger extraction, hashing, alternative topology and qualification traversal open;
two settlement dependency Clone warnings remain.

### Knowledge invalidation planning

The [knowledge planner checkpoint](formal-coverage-evidence/knowledge-plan/audit.md) connects
actual input freshness, direct decision precedence, ordered prior-dependency propagation and
complete accounting to production plan_invalidation. Parent reviewed all 12 files and actual
delta/product-resume callers, then reproduced 162 strict checks and all 14 tests. The
[bounded review](formal-coverage-evidence/knowledge-plan/review.json) explicitly keeps a full
transitive closure theorem open: constructor-enforced canonical/topological properties are not
yet part of that earlier verified contract.

The [topology followup](formal-coverage-evidence/knowledge-topology/audit.md) closes that specific
gap: successful constructor admission establishes full-byte canonical order, global uniqueness,
prior dependency membership and exact required kinds. The private snapshot invariant reaches the
actual planner, whose result now proves transitive invalidation closure; the public decision getter
also preserves the exact invalidation reason. Independent review covered all ten changed files and
reproduced 178 strict checks and all 16 tests. The
[retained review](formal-coverage-evidence/knowledge-topology/review.json) keeps full constructor
admission equivalence, lineage/time/bounds invariants, delta, authority, rendering, persistence and
end-to-end resume connections open.

The [delta-packet followup](formal-coverage-evidence/knowledge-delta/audit.md) proves exact input
admission, current freshness, prior-plan reuse and every packet delivery classification, together
with complete fields, counts and semantic cloning. Independent review covered the twelve-file
increment, with a separate Sol review of the parent-authored material helpers. Parent isolated and
integrated strict verification each passed 214 results with zero errors; all 20 package tests and
both context reuse tests passed. The [retained review](formal-coverage-evidence/knowledge-delta/review.json)
keeps exact error payloads, remaining constructor invariants, context/rendering, persistence and
end-to-end product resume open.

## Enforcement design requirements

The preferred design maintains a source-derived production/proof correspondence inventory and
checks changes using a trusted comparison base plus independently reviewed exact identities.
Actual Verus-selected items, contract strength, production callers, configuration exclusions,
assumptions, and evidence must be explicit. The credible simpler alternative is only adding
`verify-trust` to candidate CI; that detects ledger drift but leaves candidate self-authorization
and missing behavioral correspondence unresolved, so it cannot meet the goal on its own.

Required negative checks must reject broken implementations, weakened contracts, removed
obligations, verification-only substitutions, added assumptions, stale fingerprints, and missing
independent review. Pins, strict V/H no-cheating passes, bounded shards, failure propagation, and
same-run artifact custody must remain enforced. Repository rules must cover the delivery branch
and bind the authoritative check producer, not merely trust a candidate-provided success name.

The initial CI change reuses the existing strict verification jobs. Each package is verified in
its own fresh invocation, retaining compiler-selected symbols and distinct solver-query functions
with their modes. Only selected roots are cleared in a dedicated verification target; dependency
builds remain reusable. Every registered Verus evidence symbol must appear in its owning package's
output. Stale reports are removed before execution and the shard summary is published only after
every package passes. Missing/failed/partial reports, wrong tool identities, unselected registered
symbols, and foreign-root query substitutions have negative checks. Workflow policy also requires
candidate-named artifact retention. This is selection enforcement; trusted approval and complete
production correspondence are still required before the goal can finish.

Input capture now precedes all metadata used for package/target selection. Generated reports are
cleared before even policy or Cargo metadata failures. Two snapshots bind the observed Git commit,
tree, optional protected base, workspace compilation sources, manifests, shared controls, and
supported embedded data files; GitHub execution also requires clean source at the event candidate
and an existing distinct ancestor comparison base. Changed inputs invalidate the final shard summary.
These are endpoint comparisons: changed-and-restored inputs between the two observations remain a
runner-isolation boundary. Pinned external compiler/procedural-macro/dependency trust is separate.
Typed inventory parsing rejects incomplete and contradictory scope sidecars, including selected
functions or queries from a foreign root. Independent review of snapshot ordering and report
cleanup passed; it also identified and corrected the foreign selected-function case. Parent
review inspected the embedded-data scanner, manifest-context guard, and report correction, then
reran 25 proof-scope tests and five inventory tests. Manifest-relative data is permitted only with
disjoint package roots and package-local compilation sources; this is a conservative source-layout
policy, not a claimed Rust or Verus limitation. End-to-end qualification of the updated collector
passed for `app-runner`: the fresh product-runner report contains 34 solver-query functions and
both snapshots agreed over 4,428 input files. The final hashes were independently compared with
the workspace. This was an explicitly dirty local run at the audit base, with no protected-base
attestation; its successful compiler scope is not final committed CI evidence. It also reports
zero registered symbols for that package, exposing the still-missing accounting obligation records.
The full xtask suite passed 421 tests with one existing ignored test, and `xtask all` passed.
The subsequent distinct-base regression first failed and then passed after the identity gate
was corrected; the final input test group passed 16 tests and strict xtask Clippy passed.
Independent reviewer `/root/chaos_boundaries_plan` reviewed all 13 current CI files and reran
26 proof-scope tests plus five inventory tests without a blocking finding. The
[retained CI checkpoint](formal-coverage-evidence/ci-snapshot/retention.json) preserves that review
and its exact source hashes. If report-directory deletion itself fails, the job fails but the
always-running uploader can retain old files, including an old complete summary. Such artifacts
must not be accepted as successful proof evidence. The review does not close external check
producer authority or the other stated trust boundaries.

The [checker custody checkpoint](formal-coverage-evidence/ci-checker-custody/audit.md)
closes an independently found checker-source transition gap by rejecting changes to all current
repository-controlled checker build inputs before candidate metadata. Parent reviewed all nine
files and reproduced seven focused tests and the 140-reference reproducibility check. Its
[review record](formal-coverage-evidence/ci-checker-custody/review.json) explicitly leaves develop
trigger/base handling, an exclusive external check producer, bootstrap and hosted enforcement open.

The [API metadata correction](formal-coverage-evidence/api-ghost-documentation/audit.md)
admits only documented ghost-only missing-docs metadata needed by the pinned enum generator.
Both parent and independent reviewer passed 41 API tests and the actual ordinary API scan;
conditional executable attributes and public preconditions remain rejected. Its
[review](formal-coverage-evidence/api-ghost-documentation/review.json) distinguishes exact syntax
checking from the separately reviewed private-leaf placement. This later correction requires a
new final checker-input fingerprint; the earlier custody snapshot is historical evidence.

The [main/develop followup](formal-coverage-evidence/ci-main-develop/audit.md) separates the exact
workflow/checker revision, PR base and candidate. Both protected-input comparisons run before
Cargo, and proof impact remains bound to the PR base. Independent review accepted the nine-file
checkpoint, reproduced ten policy tests and the 141-reference reproducibility check, and ran
twelve local cases through the unchanged workflow identity and input guards. The
[review record](formal-coverage-evidence/ci-main-develop/review.json) keeps the external bootstrap,
exclusive App producer, final-head hosted enforcement and full runner qualification open.

The GitHub organization API reported the Team plan on 2026-09-12, with no existing team entries.
GitHub documents [required-workflow rules](https://docs.github.com/en/enterprise-cloud@latest/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets#require-workflows-to-pass-before-merging)
under its Enterprise offering; that mechanism cannot be
assumed available here. A deployable design still needs trusted check production and protected-base
review of the checker and coverage inputs. The user clarified that the existing ability to merge
their own pull requests must remain. Mandatory GitHub review/account restrictions are not part of
this implementation. No repository rules or external approval settings have been changed.

An additional ordinary feature-matrix failure was observed in unchanged agent runtime files:
`--no-default-features` disabled the optional protocol dependency while durability/driver code
still imported it or called protocol-bridge methods. The repair gates the durable driver,
durability adapters, canonical protocol codec, and their exports/tests with `protocol-bridge`.
The pure reducer, DeveloperLoop, and independent context/model/budget/tool adapters remain
available. Independent review passed with 72 default-feature tests, 64 bridge-disabled tests,
and strict Clippy in both configurations. The eight omitted tests require the protocol bridge.
The existing model-orchestration build/test/Clippy shards now run the isolated bridge-disabled
configuration after the standard all-feature command; either failure fails the shard. Parent
review checked the complete helper/integration, reran its five tests, and verified the ten-file
source manifest. The actual 19-package build shard plus additional agent build passed. No jobs
were added or timeouts increased.

## Knowledge constructor admission follow-up

The independently reviewed [knowledge constructor increment](formal-coverage-evidence/knowledge-admission/audit.md)
now proves exact production `Ok` admission, retained fields, intrinsic invariants and first-error
payload/order for limits, identities, sources, bindings, sections, current state, requests and
snapshots. Shared integration reproduced 216 strict Verus results with zero errors, 29 package
tests and both context-reuse tests. A real wrong-source error mutation failed verification and
restoration passed. The snapshot proof preserves its actual same-lineage, supplied-checkpoint
and nested-limit policies. Source review inspected exact identities and supplied evidence without
an independent compiler rerun; the parent performed integrated qualification. Planner error
payloads, complete context selection/rendering and external observation truth remain separate
work. This increment does not discharge the final register or hosted CI requirement.

## Knowledge planner error follow-up

The independently reviewed constructor-admission package now also proves exact public planner errors. `plan_invalidation` returns the first invalid clarification target with complete optional fields. `plan_delta_packet` preserves role, candidate, current-snapshot, and prior-target error precedence; its first stale current section is derived from quantified input freshness and exact actual plan prefixes. The successful planner contracts remain intact, and this increment adds no executable admission restriction.

The 38-file frozen package has manifest SHA256 `6854d6da0f6c0f6bd7b298f75b608a34e0937e32938ee266b233c6e00062e1d2`. The parent independently reviewed the eight-file increment, reran strict verification (220 verified, 0 errors) and all 29 package tests, checked all 37 shared preimages, and integrated the exact source. Integrated verification again passed 220/0, all 29 package tests, and both context-reuse tests. A supplied negative mutation replacing the stale-section payload with a plain error was rejected at the exact assertion (219/1), then restored and reverified.

Exact source identities, implementation and independent reviews, commands, raw logs, and bounded review status are retained in [knowledge planner error evidence](formal-coverage-evidence/knowledge-planner-errors/review.json). Digest authenticity, observation, persistence, rendering, context assembly, and caller-wide correspondence remain separate work. This checkpoint does not discharge the complete goal or establish final-head hosted CI success.

## Role-policy production correspondence follow-up

The role policy dependency now proves complete deterministic role mappings and policy tables, exact context-class and capability admission/error precedence, all stored projection fields, and semantic clones. The actual narrowness and reviewer-freshness predicates are connected to those production values. The 12-file increment received independent source review before integration; all 17 final package hashes match its frozen manifest.

Integrated strict verification passed 77/0, all 12 role tests passed, and both context-reuse tests passed. Appending HiddenReasoning to the actual reviewer visible table caused the expected policy postcondition failure (76/1), followed by exact restoration and77/0. Full source identities, explicit review limits, commands, and raw evidence are retained in [role-policy correspondence evidence](formal-coverage-evidence/role-policy-correspondence/review.json). This closes the role-data dependency for subsequent context proofs; downstream context enforcement and final-head CI are still pending.

## Obligation ledger and qualification correspondence follow-up

The independently reviewed obligation increment now connects the actual extraction kernel to exact source spans and retained fields, and successful qualification to input-defined counts and ordered diagnostics. Exact binary searches and reuse of previously evaluated incomplete alternative branches preserve the report while correcting a reproduced intermediate slowdown. The actual canonical encoder is verified against the existing byte format and retains the ordinary public-extraction golden digest.

After checking all 36 prior and 51 final package identities, the parent integrated 21 changed paths. Strict verification passed 356/0, all 30 obligation tests passed in default and all-features modes, and four focused gate/product-runner caller tests passed. Clippy, formatting, and ordinary API checks passed. An isolated production encoder-tag mutation failed at 355/1; exact restoration returned 356/0. The initial shared layout failure came from unfinished scheduler work, which was preserved separately before the shared scheduler was restored to its frozen checkpoint; the subsequent layout check passed.

The [retained source review and evidence](formal-coverage-evidence/obligations-ledger-qualification/review.json) distinguishes bounded runtime measurements from complexity theorems, exact successful reports from remaining complete public error contracts, and the byte encoder from the ordinary SHA-256 boundary. A separate direct universal characterization of qualification, caller observation truth, final records, and final-head CI remain work to complete.

## Release evaluator output correspondence follow-up

The [release output review](formal-coverage-evidence/release-output-correspondence/review.json) covers the actual evaluator's input-defined readiness, all 44 evidence slots and 25 criterion slots, exact canonical diagnostics, decision fields, and current fingerprint reducer. The independent reviewer checked every changed path against the corrected 52-file prior and 64-file final package identities. Parent integrated qualification passed strict Verus 377/0, 28 executable tests, and one compile-fail doc test.

The earlier incomplete preimage reconstruction was rejected and replaced with the exact retained source before review. The fingerprint proof establishes correspondence to the existing XOR reducer, with no authenticity or collision-resistance claim. Candidate-family constructors, proof-API composability, supplied observation truth, final records, and hosted qualification remain separate work.

## Release candidate constructor correspondence follow-up

The [release candidate constructor review](formal-coverage-evidence/release-candidate-construction/review.json) covers exact successful fields, nonzero admission, platform matrix slots and complete observable error kinds for the actual candidate-family constructors. The parent independently reviewed the eight-file change and all 64 prior / 65 final package identities before guarded integration. Integrated strict Verus passed 377/0; default and all-feature suites each passed 32 executable tests and one compile-fail doc test. Clippy, formatting, ordinary API and global source layout passed.

A real external client proved exact results from twelve actual derived Clone implementations. An isolated false require_revision error contract was rejected at 373/4, followed by exact restoration and successful verification. These contracts describe supplied values and input checks; they do not establish digest provenance or external platform/manifest facts. Exact diagnostic strings and public ghost-predicate composition are in a separate pending-review increment. Final global records and hosted qualification remain open.

## Release diagnostic and proof-API follow-up

The [release proof-API review](formal-coverage-evidence/release-proof-api/review.json) closes the previously recorded exact error-string and predicate-visibility gaps. The actual const error accessor now proves every existing H4 literal. Downstream proof clients can import the evaluator's existing complete input/output predicates; ordinary Rust exports and runtime behavior are unchanged. Independent review checked the exact two-file increment, all 65 final identities, and all 19 isolated evidence artifacts before guarded integration.

Integrated strict Verus passed 377/0; default and all-feature suites each passed 32 executable tests and one compile-fail doc test. Clippy, formatting, ordinary API and global source layout passed. The actual external client passed 3/0, and a wrong runtime diagnostic literal was rejected at 376/1 before restoration and 377/0. These are input-relative production-correspondence guarantees; external observation truth, remaining caller coverage, final records and hosted qualification remain open.

## Obligation qualification contract completion

The [qualification contract review](formal-coverage-evidence/obligations-qualification-contracts/review.json) closes the previously recorded exact qualification-error and direct universal-verdict gaps. The actual public boundary preserves complete existing report fields, proves first-error precedence and payloads, and connects its qualified bit to actual current evidence, resolved conditions and complete alternative branches. Independent source review and guarded integration cover all eight changed paths and 56 final package identities.

Integrated strict verification, 32 tests in each feature profile, four actual caller tests, Clippy, formatting, ordinary API and source layout all passed. The retained review separates actual payload-mutation rejection from a weakened-predicate run that hit the solver limit. Exact extraction-kernel errors, observation origin, final records and hosted CI remain separately outstanding.

## Context graph and reuse integration

The [context graph/reuse review](formal-coverage-evidence/context-graph-reuse/review.json) covers actual canonical graph admission, semantic cycle detection, exact diagnostics and reusable-section binding/visibility/provenance. Independent review rejected an intermediate cycle-traversal slowdown. A separately reviewed repair restores maintained dependent counts while proving the same elimination result; the final measured chains are faster than the prior implementation.

The parent integrated 25 paths after checking 55 prior and 72 final package identities. Integrated strict verification, both 56-test package profiles, 30 product-runner context tests, Clippy, formatting, API and layout passed. All final source identities remain intact. The raw warnings and review explicitly retain the open selection/compaction/working-state and external provenance boundaries; final records and hosted qualification are still required.

## Scheduler resource and lifecycle integration

The [scheduler integration review](formal-coverage-evidence/scheduler-resource-lifecycle/review.json) binds the independently reviewed lifecycle and resource-accounting patches to the exact combined 101-file package. Strict Verus passed 374/0, all three package test profiles passed 27/0, and Clippy, formatting, API and source layout passed. Actual daemon caller checks passed two lifecycle tests, scheduler submission/replay, and durable restart/replay after granting the test its temporary Unix-socket permission.

The final combined package passed both existing dense release fixtures, with median selection/dispatch latency at 0.677 to 0.961 times the original Git baseline. These bounded measurements recover the slowdown introduced during proof refactoring. Full command admission, cancellation/worker-loss loops, complete reducer/replay/terminal composition, final records and hosted qualification remain requirements. This checkpoint does not discharge the scheduler's broad obligations.

## Executable evidence checker integration

The [checker integration review](formal-coverage-evidence/executable-evidence-checker/review.json) records the independently reviewed repair for false rejection of actual executable Verus evidence. Exact owned declarations now supply the parsed function mode; the compiler-scope gate requires successful matching queries for executable and proof evidence. Ordinary functions, ignored tests, unsupported configuration, ambiguous ownership, and missing or mismatched queries are rejected by the regression suite.

The integrated full xtask suite passed 433 tests with no failures and no filtering. Clippy, formatting, ordinary API, source layout, documentation, and diff checks passed. The actual audit inventory now accepts 153 obligations across 66 packages, including the 16 reviewed additions, which all remain in progress. This static inventory success does not discharge obligations. A fresh compiler-scope run with this checker, final proof-impact reconciliation, and hosted CI remain required.

## Completion evidence still required

- Complete obligation-to-specification-to-production-to-caller inventory, independently reviewed.
- All feasible identified implementation and specification repairs, with rerunnable proof output.
- Precisely justified and reviewed residual boundaries and truthful obligation statuses.
- Current exact source fingerprints, affected packages, authentic reviews, and evidence records.
- Tested trusted-base CI enforcement, including adversarial negative cases and legitimate passes.
- Final commit and develop PR, with every required runner passing on that exact commit.

The goal remains active until every requirement above is evidenced; this report must be updated
as work progresses and must not be interpreted as final approval.
