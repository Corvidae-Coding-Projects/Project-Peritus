# Feature: Honest benchmark failure remediation

## Summary

This plan converts the HarnessBench and Terminal-Bench findings into general Peritus product
improvements without teaching Peritus benchmark answers or changing how the suites score it.

The most important failure is now concrete: Peritus can write a useful or even externally correct
candidate, then fail during review, provider recovery, timeout handling, or report publication and
tell neither the user nor the benchmark what it completed. The files remain in the managed
worktree, but the product exposes no durable candidate handoff because `ProductRunOutcome` only
supports `Complete` and `WaitingForUser`; all other terminal paths become errors.

The remediation separates two facts:

- **Candidate delivery:** Peritus has durably identified and handed over exact workspace changes.
- **Candidate acceptance:** current gates passed, required evidence is complete, and independent
  review has no blocking findings.

An interrupted candidate can therefore be delivered honestly as “candidate available; review
incomplete” without being falsely accepted. This is the foundation for reliable completion,
resumption, provider recovery, timeout behavior, and native benchmark reporting.

All ten slices below are required for production completion. They are not MVP stages. The waves
only define dependency order and safe parallel ownership.

## Slice index

- [S0: Verified run-settlement contract](#s0-verified-run-settlement-contract) — define and prove
  candidate, evidence, qualification, and terminal-state semantics.
- [S1: Digest-bound grounded run knowledge](#s1-digest-bound-grounded-run-knowledge) — reuse
  current repository knowledge without trusting stale summaries.
- [S2: Typed requirements and failure ownership](#s2-typed-requirements-evidence-obligations-and-failure-ownership)
  — enforce evidence-bearing requirements and route only code defects to the fixer.
- [S3: Adapter handshake and terminal reporting](#s3-external-adapter-handshake-and-terminal-reporting)
  — guarantee honest native reports for admitted external trials.
- [S4: Provider and command recovery](#s4-provider-credential-context-and-command-recovery) —
  normalize provider failures, preserve credentials, and qualify active commands.
- [S5: Product-runner integration](#s5-product-runner-checkpoint-resume-and-finalization-integration)
  — checkpoint candidates, reserve finalization time, and resume incomplete phases.
- [S6: Daemon and TUI handoff](#s6-durable-daemon-state-and-candidate-handoff-ux) — persist and
  present accepted and unqualified candidates clearly.
- [S7: General capability fixtures](#s7-general-capability-qualification-fixtures) — reproduce
  every remediable class without benchmark-specific logic.
- [S8: Product and release qualification](#s8-integrated-product-and-release-qualification) —
  assemble, test, and freeze one provenance-bound candidate.
- [S9: External qualification](#s9-frozen-external-qualification-and-final-report) — run unchanged
  suites once and publish every result.

## Proposed solutions at a glance

| Observed problem | Proposed general solution | Delivery slice |
| --- | --- | --- |
| Work exists but Peritus does not turn it in | Verified settlement states and durable candidate checkpoints | S0, S5, S6 |
| Later failure erases the meaning of earlier work | Finalization runs on every exit and publishes the strongest current checkpoint | S5 |
| Design, source reads, and orchestration repeat across rounds | Digest-bound grounded run knowledge with selective invalidation | S1, S5 |
| Provider stalls, malformed responses, or context overflow end the whole run | Phase-local recovery and resume from the first stale phase | S4, S5 |
| Fixer loops attack infrastructure or ambiguity as though it were bad code | Typed failure ownership: candidate, contract, provider, harness, or evaluator | S2, S5 |
| Performance changes are accepted without measurements | Typed same-workload baseline/candidate evidence | S2, S7 |
| Internal simulation substitutes for a requested real lifecycle boundary | Typed signal/restart/disconnect/crash ingress evidence | S2, S7 |
| Request and response contracts are confused | Directional schema obligations and bidirectional tests | S2, S7 |
| Hand parsers claim browser-equivalent behavior | Browser-semantics obligation backed by a standards implementation | S2, S7 |
| Image tasks reach an image-incapable provider | Provider capability preflight before role assignment | S4 |
| Timeouts occur before checks or reporting can finish | Phase budgets plus a protected finalization reserve | S5 |
| Native reports are missing or use stale schemas | Adapter handshake and unconditional terminal report settlement | S3 |
| Credential rotation repeatedly loses a valid login | Preserve future-valid rotated credentials and require a real canary | S4 |
| Interactive terminal support was absent in the frozen build | Qualify the already-landed C4/C2 command lifecycle; do not rebuild it | S4, S7 |
| Fixed-build results cannot be reproduced | Mandatory source revision, binary digest, configuration digest, and suite pins | S3, S8, S9 |

## Implementation roadmap

```text
Wave A: freeze pure contracts
  S0 Settlement contract
  S1 Grounded run knowledge        } parallel after their type skeletons are agreed
  S2 Acceptance obligations        }

Wave B: harden independent effect boundaries
  S3 Adapter and native reporting  } parallel; no shared production paths
  S4 Provider and command recovery }

Wave C: assemble the product
  S5 Product-runner integration -> S6 Daemon and TUI handoff

Wave D: prove the integrated behavior
  S7 Capability qualification -> S8 Product/release qualification

Wave E: measure, without changing code
  S9 Frozen external benchmark qualification
```

Safe parallel groupings are S1+S2, S3+S4, and selected S7 fixture families. S5 is deliberately the
single integration owner for `peritus-product-runner`; this prevents several agents from colliding
in its composition files. S9 starts only after the production source and release binary are frozen.

## Implementation slices

### S0: Verified run-settlement contract

**Purpose:** Define one unambiguous answer to “what did this run finish, and what may Peritus claim
about it?” before effectful crates are changed.

**Owns:**

- New V-class crate `crates/orchestration/peritus-run-settlement`.
- Run-disposition and candidate-qualification additions in `peritus-app-protocol`.
- Wire tags, codecs, compatibility fixtures, and generated protocol artifacts for those additions.

**Implementation:**

1. Add `CandidateIdentity` containing run ID, workspace ID, candidate digest, conversation
   revision, and checkpoint sequence.
2. Add `CandidateStage`: observed, changed, self-checked, gates-passed, review-pending, qualified.
3. Add `EvidenceStatus<T>`: missing, current, failed, or stale with typed provenance.
4. Add `RunDisposition`: accepted, candidate-available, waiting-for-user, failed-no-candidate,
   cancelled, or recovery-required.
5. Add `SettlementCause`: completed, user-wait, cancellation, deadline, provider, context, gate,
   review, repository, adapter, or internal invariant.
6. Add a pure reducer that accepts observations and returns the only legal disposition.
7. Separate automated qualification from the existing `ProductDeliverable.accepted` user choice.
   The existing Boolean continues to mean that the user accepted the handoff.
8. Decode legacy deliverables as qualified because the current daemon only creates them after E0
   acceptance. Never reinterpret old wire tags.

**Verus work:** Prove that accepted implies a current candidate, current passing gates, satisfied
required obligations, and a current blocker-free review; candidate-available never implies
accepted; stale evidence cannot cross candidate revisions; checkpoint strength is monotonic; and a
terminal settlement cannot be replaced by a contradictory terminal settlement.

**Tests:** Complete reducer transition matrix, stale evidence, duplicate settlement, legacy decode,
new encode/decode, unknown tag, collection bounds, and generated-schema consistency.

**Done when:** Effectful callers can express every terminal benchmark condition without strings or
loss of candidate state, and `cargo verus verify --package peritus-run-settlement` passes with
`--no-cheating`.

**Dependencies:** None. S0 freezes the interfaces consumed by S3, S5, and S6.

### S1: Digest-bound grounded run knowledge

**Purpose:** Stop repeating design, full repository reads, and unchanged reasoning while ensuring
the model remains grounded in the current workspace rather than a stale summary.

**Owns:**

- New V-class crate `crates/orchestration/peritus-run-knowledge`.
- Focused extensions to `peritus-context` for reusable, provenance-bearing selections.
- No edits to `peritus-product-runner`; S5 performs that integration.

**Implementation:**

1. Define `RunKnowledgeSnapshot` with repository inventory, relevant-file map, literal requirement
   ledger reference, design sections, compacted tool observations, resolved findings, and candidate
   evidence index.
2. Bind each section to workspace identity, source content digests, conversation revision,
   candidate revision, role, and creation sequence.
3. Add a pure invalidation planner. A user clarification invalidates affected requirement/design
   sections; a changed file invalidates its source observations and dependent evidence; a provider
   failure invalidates no repository facts by itself.
4. Add delta-packet planning for writer, reviewer, and fixer roles. The packet contains the changed
   authoritative facts plus references to still-current facts.
5. Retain model-authored summaries only as navigation text. They never establish a file's contents,
   a passing test, a resolved finding, or an acceptance fact.
6. Record reuse and invalidation counts so the product can show whether compaction and caching
   actually avoided work.

**Verus work:** Prove that the reuse planner selects only current observations, invalidation is
monotonic with respect to changed inputs, and no summary can satisfy an authoritative evidence
requirement.

**Tests:** Same-revision reuse, one-file invalidation, conversation-delta invalidation, candidate
revision invalidation, provider retry with full reuse, role isolation, oversized inventory, and
deterministic rendering.

**Done when:** A repeated-round fixture proves that unchanged inventory, design, and source evidence
are reused, while one changed source file is reread before it influences a new decision.

**Dependencies:** Uses S0 identity vocabulary but can be implemented in parallel with S2 after the
shared identity skeleton is frozen.

### S2: Typed requirements, evidence obligations, and failure ownership

**Purpose:** Turn public task requirements into enforceable evidence instead of relying only on
prompt reminders, and stop sending non-code failures through the fixer loop.

**Owns:**

- New V-class crate `crates/orchestration/peritus-obligations`.
- Pure obligation/evidence additions in `peritus-gates`.
- Generic fixtures for obligation extraction. Product-runner prompt and loop integration belongs
  to S5.

**Implementation:**

1. Define a `RequirementLedger` whose entries retain exact public source clauses and provenance.
2. Classify entries as hard, conditional, alternative, example, generated output, performance,
   lifecycle ingress, request schema, response schema, browser semantics, or external effect.
3. Preserve the current explicit-path distinctions instead of treating every mentioned path as a
   mandatory output.
4. Define `PerformanceEvidence`: workload identity, baseline, candidate, repetitions, statistic,
   noise margin, and public threshold.
5. Define `LifecycleEvidence`: named public ingress, control event, observed process/service
   transition, and final state. Internal simulation is supplementary only.
6. Define directional schema obligations so request and response fields cannot be inferred from one
   another.
7. Define browser-semantics obligations that require a standards-compliant implementation when the
   task claims browser behavior.
8. Define failure ownership: `CandidateDefect`, `ContractAmbiguity`, `ProviderFailure`,
   `HarnessInfrastructure`, or `ExternalEvaluator`.
9. Permit only `CandidateDefect` to request another fixer cycle. Ambiguity may ask one material
   question; infrastructure and provider failures recover or settle.

**Verus work:** Prove that every required obligation has current evidence before qualification,
alternatives require at least one complete branch, conditional obligations activate only when
their public condition holds, and non-candidate failures cannot authorize a fixer transition.

**Tests:** Performance evidence absent/present/stale, internal versus real lifecycle ingress,
asymmetric `value`/`val` request-response schemas, malformed HTML with and without a browser oracle,
conditional paths, alternatives, and failure-owner transition matrix.

**Done when:** The four currently visible capability misses—unmeasured performance, simulated
lifecycle, directional schema confusion, and non-browser parser behavior—are detected by generic
fixtures containing no benchmark IDs or copied verifier logic.

**Dependencies:** S0 identity vocabulary. Parallel with S1.

### S3: External adapter handshake and terminal reporting

**Purpose:** Ensure every native agent invocation that starts produces one honest terminal report,
even when the runner, provider, timeout, or report projection fails.

**Owns:**

- `crates/app/testing/peritus-external-benchmarks`.
- `benchmarks/external/harnessbench` and `benchmarks/external/terminalbench` adapter code.
- New benchmark-report schema and adapter fixtures.
- No production runner, daemon, provider, or TUI paths.

**Implementation:**

1. Add a pre-run handshake covering adapter schema, product protocol version, provider route,
   workspace availability, trace path, evidence path, and source/binary identity.
2. Prepare and validate workspace, trace, and report directories before admitting a trial.
3. Install an unconditional settlement guard after admission. It writes exactly one atomic native
   report on success, error, timeout, cancellation, or unwinding at the adapter boundary.
4. Add report fields for native disposition, candidate stage, candidate digest, changed paths,
   gate/review/obligation status, terminal cause, source revision, binary SHA-256, configuration
   digest, suite revision, elapsed time, and resources.
5. Keep official reward, verifier exception, and native disposition independent.
6. Preserve native `success = true` only for strict `Accepted`. `CandidateAvailable` never changes
   upstream reward or becomes a native success.
7. Make stale schemas, missing `/app`, missing trace setup, and report-publication faults distinct
   adapter classifications.
8. Retain a recovery record if final atomic publication itself fails.

**Tests:** Unsupported schema, missing workspace, unwritable trace path, provider error, runner
timeout, cancellation, report write failure, duplicate-finalization attempt, source identity absent,
and accepted/candidate/external-reward cross-product.

**Done when:** Every admitted fault-injection trial produces exactly one parseable report or one
durable recovery record, and no report claims success without an accepted settlement.

**Dependencies:** S0 protocol types. Parallel with S4.

### S4: Provider, credential, context, and command recovery

**Purpose:** Make provider and execution failures phase-local so a stalled model or command does not
erase an already useful candidate.

**Owns:**

- Provider-core recovery and capability interfaces.
- Codex, Claude, OpenAI, Anthropic, Google, and compatible-provider adapter tests and narrowly
  required fixes.
- Existing C4/C2 active-command qualification fixtures.
- No product-runner composition; S5 consumes the resulting interfaces.

**Implementation:**

1. Declare role capabilities before invocation: text, image input, maximum context, tool protocol,
   account/API route, and current authenticated availability.
2. Reject an incapable route before spending a model turn; select a user-authorized capable
   fallback when available.
3. Normalize empty terminal responses, malformed responses, context overflow, ambiguous
   acceptance, authentication failure, capacity, and subprocess timeout into distinct typed causes.
4. Preserve the existing run-scoped provider circuit and consented failover.
5. Preserve a newly rotated, future-valid credential when the host credential file has not
   independently changed.
6. Require one minimal real provider request after login or credential repair before marking the
   route available for expensive work.
7. Validate that bounded compaction is attempted according to policy, but return a context cause
   rather than blindly restarting the whole run when the request remains too large.
8. Qualify the existing `command_start`, poll, stdin, resize, signal, cancel, recover, timeout, and
   process-tree reaping path added in `e9da73a0`; add no duplicate PTY stack.

**Tests:** Capability routing, image mismatch, empty terminal, malformed terminal, over-limit after
compaction, failover consent, circuit opening, login rotation, host-file change, real canary,
command recovery, signal delivery, cancellation, timeout, and no surviving descendants.

**Done when:** Every provider/command terminal has a stable cause and retry disposition, fresh
logins survive rotation, image tasks never reach text-only routes, and active commands settle their
owned process trees.

**Dependencies:** Can proceed alongside S3. S5 later binds these causes to candidate settlement.

### S5: Product-runner checkpoint, resume, and finalization integration

**Purpose:** Make the writer-reviewer-fixer loop recognize completed work, preserve it, and stop
repeating phases that are already valid.

**Owns:**

- All changes under `crates/app/peritus-product-runner` for this remediation wave.
- New focused modules `execution/checkpoint.rs`, `execution/deadline.rs`,
  `execution/resume.rs`, `execution/settlement.rs`, and `execution/obligations.rs`.
- This slice is the sole integration owner for product-runner manifests, roots, prompts, and
  composition files.

**Implementation:**

1. Replace the success-or-error terminal shape with `RunSettlement` from S0. Keep errors only for
   invalid preconditions and impossible internal invariants.
2. Capture a candidate checkpoint after every material mutation, successful verification command,
   deterministic gate result, admitted review, and fixer cycle.
3. Compute candidate identity from the exact managed workspace state and conversation revision.
4. Run a finalization arbiter on every ordinary exit path. It refreshes the diff, identifies the
   strongest candidate, marks evidence current/stale, and returns the honest disposition.
5. Attach incomplete evidence and remaining work to `CandidateAvailable` rather than discarding it
   behind `ProductRunnerError`.
6. Add phase-aware budgets and a protected finalization reserve. Stop starting open-ended model
   turns before that reserve is consumed.
7. Resume at the first stale or missing phase. A reviewer failure reruns review; it does not rerun
   design and writer. A report failure reruns report publication only.
8. Consume S1 knowledge snapshots for digest-valid design, inventory, source, and finding reuse.
9. Consume S2 obligations in gates, reviewer input, acceptance, and fixer routing.
10. Consume S4 typed provider and command causes without converting them to candidate defects.
11. Preserve the current strict acceptance predicate: exact gates pass, obligations are satisfied,
    and current independent review has no blockers.

**Tests:** Failure injection before mutation, after mutation, after gates, during review, during a
fix, at the finalization cutoff, and during cancellation; phase invocation counters; stale
checkpoint rejection; conversation change; provider recovery; candidate with failing gates;
candidate with pending review; and accepted candidate.

**Done when:** Any terminal path after a real workspace mutation returns a candidate handoff unless
a fresh diff proves no candidate exists, and counter-based tests prove valid earlier phases are not
repeated.

**Dependencies:** S0, S1, S2, and S4. S5 is sequential integration work, not a parallel worker
cluster.

### S6: Durable daemon state and candidate-handoff UX

**Purpose:** Persist the runner's richer truth and make it understandable and actionable from the
single-command product.

**Owns:**

- `crates/app/peritus-daemon/src/product_run` execution, snapshot, persistence, lifecycle, and
  deliverable modules.
- Product-run presentation and actions in `peritus-tui` and the CLI client.
- No changes to S0 wire definitions or S5 runner internals.

**Implementation:**

1. Persist each checkpoint as the observer receives it using the existing durable record pattern.
2. Attach a `ProductDeliverable` to both accepted and candidate-available terminal snapshots.
3. Persist qualification separately from user accept/commit/export/discard decisions.
4. On restart, validate workspace and candidate digests, mark stale evidence, and resume from the
   first missing phase.
5. Render clear states: Accepted, Candidate available, Waiting for you, Stopped with no candidate,
   Cancelled, and Recovery required.
6. Show exact changed paths, successful/failed/missing checks, review state, remaining work, run
   instructions, and interruption cause.
7. Offer inspect, run, continue, export, accept, commit, and discard actions for the exact candidate.
8. Require explicit confirmation before accepting or committing an unqualified candidate, naming
   the failed or missing evidence. Export and inspection remain available.

**Tests:** Daemon restart at each checkpoint stage, legacy record migration, stale workspace,
candidate available after reviewer loss, candidate after cancellation, action targeting by digest,
double action, TUI rendering snapshots, and end-to-end continue-to-accept.

**Done when:** A user can launch `peritus`, watch an interrupted run become a visible candidate,
inspect what remains, continue it, and receive an accepted deliverable without searching internal
worktree paths or logs.

**Dependencies:** S0 and S5.

### S7: General capability qualification fixtures

**Purpose:** Prove the systemic changes against generic tasks that reproduce the failure forms
without copying benchmark IDs, expected outputs, or hidden verifier rules.

**Owns:**

- Focused additions to existing performance, resilience, platform, and external-adapter
  qualification crates.
- New generic fixture trees under their owning testing crates.
- No production logic.

**Implementation and fixture families:**

1. **Completion:** artifact written, reviewer unavailable; candidate must be delivered unaccepted.
2. **Resume:** design/write/gates complete, later phase fails; only the failed phase reruns.
3. **Performance:** a plausible optimization that is measurably slower; acceptance must block.
4. **Lifecycle:** internal cancellation simulation versus a real process signal and observed exit.
5. **Schema:** request uses `value`, response uses `val`; the directional contract must remain exact.
6. **Browser semantics:** malformed HTML differs between a hand parser and standards parser.
7. **Provider:** image-capability mismatch, empty response, context overflow, authentication repair,
   fallback, and circuit behavior.
8. **Repository:** large inventory, nested Git, task-authorized HEAD mutation, and external drift.
9. **Prerequisites:** missing ordinary compiler/runtime in an authorized disposable environment.
10. **Terminal:** interactive input, resize, signal, cancellation, recovery, and child reaping.
11. **Adapter:** stale schema, absent workspace/trace directory, timeout, and publication failure.

**Tests:** Each family includes success, honest partial-delivery, and failure cases. Assertions target
public behavior and typed evidence, not model prose.

**Done when:** Every remediable failure class in the traceability table below has at least one
deterministic focused regression and the full fixture matrix passes under bounded resources.

**Dependencies:** S3 through S6. Fixture families may be authored in parallel by non-overlapping
test-crate ownership; one integrator runs and assembles them.

### S8: Integrated product and release qualification

**Purpose:** Establish that all remediation code works together and produces one reproducible
release candidate before expensive external suites run.

**Owns:**

- Integration tests, protocol/schema regeneration, documentation, release evidence, and CI
  workflow adjustments required by the new crates.
- No new feature behavior except root-cause fixes for integration failures.

**Implementation:**

1. Run focused package tests and formal proofs for S0–S7.
2. Run architecture, dependency-direction, source-size, generated-asset, docs, formatting, Clippy,
   and Rust test gates.
3. Run real inert Codex and Claude canaries after confirming both routes are logged in.
4. Run a complete local product scenario: start Peritus, create a generic Rust application,
   interrupt review, inspect the candidate, continue, run it, and accept it.
5. Build one release binary, record its Git revision, SHA-256, configuration digest, provider
   profile, and toolchain versions, and prohibit source changes after freezing it.
6. Keep hosted CI jobs sharded under ten minutes; do not replace useful checks with longer single
   jobs.

**Tests:** Complete local release gate plus Linux/macOS/Windows hosted shards applicable to the
changed boundaries.

**Done when:** One immutable release candidate passes all production gates and its provenance is
complete enough for S9 to reject any different binary or source tree.

**Dependencies:** S7.

### S9: Frozen external qualification and final report

**Purpose:** Measure the real effect of the remediation once, honestly, without changing product
code between tasks or rerolling failures.

**Owns:**

- Generated evidence outside Git.
- Final human-readable benchmark report and checked-in aggregate summaries.
- No production, prompt, adapter, fixture, timeout, or verifier changes during the campaign.

**Implementation:**

1. Pin the immutable S8 binary, source revision, provider configuration, HarnessBench revision,
   Harbor revision, and Terminal-Bench revision.
2. Run HarnessBench and Terminal-Bench unchanged with the documented resource-aware settings.
3. Preserve every attempt, reward, native disposition, candidate state, exception, trace, and
   resource record.
4. Compare the frozen diagnostic baseline with the new campaign by failure class, not only score.
5. Report official metrics first, then native/external agreement, delivery rate, acceptance rate,
   missing-report rate, failure classes, resource use, and confidence limits.
6. Document every retained oracle/fixture defect and explain why special-casing it would be
   benchmark cooking.
7. Do not rerun a failed trial merely to select a luckier result. A new campaign requires a new
   declared build and keeps the prior campaign.

**Tests:** Aggregate integrity, expected task/trial counts, unique identity, source/binary binding,
raw-attempt preservation, and report-schema validation.

**Done when:** Both complete campaigns and the final report exist, every adverse result remains
accounted for, and no result is presented as evidence for a different build.

**Dependencies:** S8. S9 is measurement, not another code-remediation loop.

## Failure-class traceability

| Failure class | Diagnostic evidence | Primary slices | Honest closure |
| --- | --- | --- | --- |
| Repeated orchestration and source reads | `HBE-001`; worst retained run 139 requests, about 2.39 million tokens, 1,417.6 seconds | S1, S5, S7 | Counter-based repeat-round fixture and immutable campaign |
| Harness product findings with source corrections | `HBF-001`–`HBF-030`, `HBA-001`, `HBS-001`, `HBC-001`, `HBM-001`, `HFC-001`, `HBT-001` | S5, S7, S9 | Preserve fixes, add missing focused regressions, qualify one build |
| Harness adapter/integration faults | `HBI-001`, `HBI-004`, `HBI-027` | S3, S7, S9 | Adapter fault matrix and immutable campaign |
| Harness later-task generalizations | `HBI-048`–`HBI-052`, `HBI-055`–`HBI-059`, `HBI-061`, `HBI-062`, `HBI-065` | S1, S2, S5, S7 | Public-contract regressions and immutable campaign |
| Candidate produced but native completion lost | 141 reward-one trials without clean native completion; 66 missing native reports | S0, S3, S5, S6 | Candidate handoff and exactly-one-report tests |
| No usable provider response | 175 native rejections | S4, S5 | Typed recovery matrix; preserve candidate when present |
| Image-capability mismatch | 14 native rejections | S4, S7 | Capability routing before invocation |
| Context overflow after compaction | 21 native rejections | S1, S4, S5, S7 | Delta context, phase resume, and honest settlement |
| Gate/fixer no progress | 18 native rejections | S2, S5, S7 | Fix only candidate defects; deliver unresolved candidate |
| Repository/workspace failures | 8 native rejections across inventory and HEAD mutation | S5, S7 | Focused repository matrix and checkpoint preservation |
| Unnecessary user escalation | 4 native rejections | S2, S5, S7 | Capability/prerequisite detection and question validation |
| Provider auth, ambiguous acceptance, subprocess timeout | 5 native rejections | S4, S5, S7 | Credential lifecycle, terminal normalization, settlement |
| Agent deadline | 56 Harbor exceptions | S3, S5, S7 | Phase budget and protected finalization reserve |
| Adapter/runtime setup | 45 runtime exceptions: 29 schema, 3 router, 3 workspace, 9 trace, 1 process tree | S3, S4, S7 | Preflight, unconditional report, and process settlement |
| Verifier timeout | 7 Harbor exceptions | S3, S9 | Preserve native truth and external unscored status separately |
| Unmeasured performance claim | `TBF-037` | S2, S5, S7 | Typed comparative evidence required for acceptance |
| Simulated public lifecycle | `TBF-041` | S2, S5, S7 | Real-ingress evidence required when publicly named |
| Interactive terminal lifecycle | `TBF-047`; implementation landed in `e9da73a0` | S4, S7, S9 | Qualify existing lifecycle; do not duplicate it |
| Directionally wrong schema | `TBM-001` | S2, S7 | Generic asymmetric request/response fixture |
| Non-browser-equivalent parser | `TBM-002` | S2, S7 | Generic standards-browser fixture |
| Missing reproducible identity | Legacy `allow_legacy` reports | S3, S8, S9 | New campaigns fail closed without source and binary identity |

The retained evaluator and fixture defects `HBI-002`, `HBI-003`, `HBI-005`–`HBI-026`,
`HBI-028`–`HBI-047`, `HBI-053`, `HBI-054`, `HBI-063`, `HBI-066`, `HBI-067`, and
`TBI-004`, `TBI-005`, `TBI-007`–`TBI-011` are deliberately not assigned product-remediation
slices. They remain in raw scores and may be reported upstream. Encoding their unpublished or
contradictory rules would be benchmark cooking.

## User-visible behavior

- A run that changes files always ends with either an accepted deliverable or a visible candidate
  explaining what remains unverified.
- Candidate handoffs show exact changed paths, checks completed, checks failed or missing, review
  status, run instructions, and terminal cause.
- Continuing a candidate resumes the first stale or missing phase rather than starting over.
- Provider, context, timeout, and adapter failures remain distinct instead of appearing as a vague
  “the agent failed.”
- The user can inspect, run, continue, export, accept, commit, or discard the exact candidate.
- The ordinary `peritus` command remains the complete product entry point. No environment exports
  or benchmark-specific modes enter the normal experience.

## Requirements

1. Preserve the strongest exact candidate after every material effect boundary.
2. Bind candidate and evidence to run, workspace, candidate digest, conversation revision, and
   checkpoint sequence.
3. Keep candidate delivery and automated acceptance as different typed facts.
4. Settle every admitted run exactly once, including provider, timeout, cancellation, and error
   paths.
5. Reserve sufficient bounded time for candidate capture, process settlement, persistence, and
   report publication.
6. Reuse only digest-current grounded knowledge and invalidate affected sections deterministically.
7. Route only candidate defects to the fixer.
8. Enforce performance, lifecycle, directional schema, browser behavior, and external-effect
   obligations from public requirements.
9. Capability-check providers before assigning roles and verify repaired credentials with a real
   canary.
10. Publish source revision and binary SHA-256 for every fixed-build report.
11. Preserve raw benchmark scores, attempts, failures, and exceptions.
12. Keep production Rust source files under 500 lines, crate roots thin, and dependencies directed
    inward according to the existing architecture policy.
13. Implement every deterministic, supported decision in Verus Rust. Record a concrete exclusion
    reason and compensating test for effectful or unsupported boundaries.
14. Contain benchmark code in testing/adapter crates; production code must not know task IDs or
    verifier details.

## Acceptance criteria

1. A run that writes a candidate and loses its reviewer produces `CandidateAvailable` with
   `ReviewPending`, exact paths, and a non-success native report.
2. A fully checked, blocker-free run produces `Accepted`, and the same candidate digest appears in
   checkpoint, daemon snapshot, deliverable, trace, and adapter report.
3. A post-mutation provider error, context overflow, deadline, cancellation, or no-progress stop
   never becomes failed-no-candidate unless a fresh diff proves no candidate exists.
4. Resume counters prove that valid design, inventory, writer, gate, and review phases are not
   repeated.
5. Evidence from an older candidate or conversation revision cannot authorize a newer candidate.
6. Missing comparative performance evidence blocks a performance claim; current satisfying
   evidence permits it.
7. Internal lifecycle simulation cannot satisfy a named public signal/restart/disconnect/crash
   requirement; a real public-boundary observation can.
8. Generic asymmetric schema and malformed-HTML fixtures detect the two candidate-capability misses
   without benchmark identifiers.
9. Every admitted benchmark invocation produces exactly one terminal report or a durable recovery
   record.
10. Active command poll/input/resize/signal/cancel/recovery/reaping passes without a second PTY
    implementation.
11. Fixed-build campaign tooling rejects missing source/binary identity and preserves every raw
    attempt.
12. Formal proofs, focused tests, architecture/docs/source-size checks, strict Clippy, Rust tests,
    and sub-ten-minute hosted shards pass.

### Requirement-to-evidence map

| Requirements | Acceptance evidence | Owning slices |
| --- | --- | --- |
| 1–4: checkpoint identity, delivery/acceptance separation, exactly-once settlement | Criteria 1–3 and 9 | S0, S3, S5, S6 |
| 5: finalization reserve | Criteria 3 and 9; clock-controlled deadline tests | S3, S5, S7 |
| 6: grounded reuse and invalidation | Criteria 4 and 5; phase counters and stale-evidence tests | S1, S5, S7 |
| 7–8: failure ownership and typed obligations | Criteria 6–8 | S2, S5, S7 |
| 9: provider capability and credential canary | Criteria 3 and 10; provider recovery matrix | S4, S5, S7 |
| 10–11: identity and raw evidence preservation | Criteria 9 and 11 | S3, S8, S9 |
| 12–13: maintainable Verus-first Rust | Criterion 12 | S0–S8 |
| 14: benchmark isolation | Criteria 8 and 11 plus task-ID scan | S2, S3, S7, S9 |

## Current architecture

`peritus-product-runner/src/execution.rs` already composes design, writer, exact-target gates,
independent review, and fixer cycles. Its `ProductRunOutcome` is currently only
`Complete(ProductRunOutput)` or `WaitingForUser`; provider, context, gate, repository, budget, and
review terminals escape as `ProductRunnerError`.

`peritus-daemon/src/product_run/execution.rs` creates a `ProductDeliverable` only for `Complete`.
Errors replace the snapshot with `Failed` and append “I couldn't finish this run.” The worktree may
still contain correct files, but no typed deliverable exposes them.

`peritus-app-protocol::ProductDeliverable` carries paths, successful commands, and run instructions.
Its `accepted` Boolean means the user accepted the handoff, not that gates/review accepted the
candidate. The new schema must preserve that meaning and add explicit qualification.

The current tree already has bounded compaction, cache accounting, per-command completion reserve,
provider failover, credential rotation logic, a run-scoped provider circuit, exact-path gates,
process-tree cleanup, grounding requirements, and active command handles. Developer instructions
already request same-workload performance measurement. The plan integrates and qualifies these
capabilities rather than pretending they are absent.

The diagnostic evidence remains:

- HarnessBench: 106 tasks, mean outcome `0.89685`, mean process `0.92865`, security `1.0`; complete
  diagnostic aggregate across an evolving build.
- Terminal-Bench: 445 trials, 239 reward one, 151 reward zero, 55 unscored; 134 native accepted,
  245 rejected, 66 missing reports.
- Only 98 Terminal-Bench trials were simultaneously reward one, natively accepted, and
  exception-free.

## Proposed design

### Settlement flow

```text
No candidate
    |
    | workspace mutation or requested external effect
    v
Candidate checkpoint ---- provider/context/deadline/cancel ----> Candidate available
    |
    | exact gates produce current evidence
    v
Gates checked -------- gate failure after bounded fixes -------> Candidate available
    |
    | independent current review
    v
Review checked ------- reviewer unavailable/blockers ----------> Candidate available
    |
    | all public obligations current and satisfied
    v
Accepted -------------------------------------------------------> Durable deliverable
```

The arrows to `Candidate available` are terminal handoffs, not successes. A later continuation
starts at the first stale or incomplete node. `FailedNoCandidate` is reserved for runs that never
created a candidate or whose fresh workspace observation proves none exists.

### Authority and data flow

1. Effectful runner code observes workspace, command, provider, gate, and review results.
2. Pure verified crates decide evidence freshness, obligations, permitted recovery, and settlement.
3. The runner emits monotonic checkpoints through `RunObserver`.
4. The daemon persists the checkpoint before presenting it.
5. App protocol transports typed disposition and qualification.
6. TUI renders facts and actions without re-parsing status strings.
7. External adapters project the same settlement into reports while leaving external reward
   untouched.

### Preferred design versus alternatives

| Design | Operational result | Decision |
| --- | --- | --- |
| Dual-axis settlement with monotonic checkpoints | Honest candidate handoff, strict acceptance, phase resume, general product value | Preferred |
| Treat external verifier pass as native completion | Raises apparent agreement but makes benchmark internals authoritative | Rejected as cooking |
| Skip review near deadline | Produces more “successes” by weakening the product contract | Rejected |
| Leave candidate only in a hidden worktree | Avoids schema work but remains unusable and unresumable | Rejected |
| Save one checkpoint only immediately before success | Cannot survive the failures observed during writing, gates, or review | Rejected |
| Increase timeouts globally | Hides scheduling defects and still does not guarantee settlement | Rejected |

## Data and compatibility

- Add a new app-protocol schema version and new stable tags. Do not repurpose existing tags.
- Legacy deliverables decode as qualified because old code created them only after strict E0
  completion. Their user accepted/committed/exported/discarded state remains independent.
- Persist bounded, versioned checkpoints with write-sync-rename.
- Candidate identity uses canonical relative paths and exact workspace state; absolute workspace
  paths remain local presentation data.
- Run-knowledge snapshots are content-addressed caches. Decode or freshness failure discards the
  cache, never the candidate.
- Frozen benchmark v1 records remain immutable. The fixed campaign writes a new identity-required
  schema.

## Failure handling

| Failure | Required behavior |
| --- | --- |
| Provider unavailable before mutation | Failed-no-candidate with provider cause and recovery action |
| Provider unavailable after mutation | Candidate available with completed evidence preserved |
| Context still too large after bounded compaction | Settle current candidate; do not restart the full run blindly |
| Gate failure | Candidate available with exact failing commands after bounded useful fixes |
| Reviewer unavailable | Candidate available with review pending; never accepted |
| Contract ambiguity | Ask one material question and attach the candidate when present |
| Deadline | Stop new model work at reserve, capture candidate, settle processes, persist, report |
| Cancellation | Cancel and reap effects, preserve stopped candidate, label cancelled |
| Daemon restart | Recover checkpoint/handles, validate digests, resume first stale phase |
| External workspace drift | Preserve identities, invalidate stale evidence, require reconciliation |
| Native report failure | Write recovery record and exit nonzero; never fabricate success |
| External verifier failure/timeout | Preserve native settlement and raw external exception separately |

## Security considerations

Candidate delivery grants no merge, commit, push, external-effect, or acceptance authority. Existing
workspace confinement, structured command routing, provider consent, credential ownership, and
process ownership remain active. Checkpoints store digests and evidence references, never provider
credentials. Reused context retains provenance and becomes stale when its authority source changes.

The plan addresses observed integrity problems—stale evidence, ambiguous status, credential
rotation, missing reports, and leaked processes—without expanding into speculative threats that do
not affect this application.

## Verification

### Before external suites

1. Run each slice's focused unit, integration, migration, fault-injection, and Verus tests.
2. Run architecture, dependency, source-size, generated-file, documentation, format, Clippy, and
   Rust test gates with memory-aware build concurrency.
3. Keep CI checks sharded so each hosted job remains under ten minutes.
4. Run minimal real Codex and Claude canaries after login; do not expose credentials.
5. Run the end-to-end product scenario from S8 using the same release binary intended for S9.

### External qualification

1. Freeze source, binary, provider configuration, and upstream suite revisions.
2. Run each complete suite unchanged once under its documented attempt policy.
3. Validate expected task/trial counts and immutable identity on every record.
4. Publish raw official results before any diagnostic grouping.
5. Compare failure-class counts, native/external agreement, candidate delivery, acceptance, missing
   reports, resources, and uncertainty.

A target score is deliberately not an acceptance criterion. General mechanisms and honest complete
measurement are. A higher legitimate score is the expected consequence, not a rule embedded in the
product.

## Rollout and rollback

1. Land S0 protocol readers before new writers and preserve legacy fixtures.
2. Land S1–S4 independent foundations behind unused APIs.
3. Land S5 runner integration and validate settlements before enabling new UI states.
4. Land S6 persistence and UX, keeping legacy accepted-run display compatible.
5. Land S7 regressions and S8 release qualification.
6. Freeze the binary, then run S9 without production changes.

Rollback may disable new presentation or resume behavior, but it must keep checkpoints readable and
candidate worktrees recoverable. It must never delete candidates, reports, or raw benchmark
evidence. Protocol writers may use a legacy form only when doing so cannot hide an unqualified
candidate.

## Open questions

- Final user-facing label for an unqualified candidate. The proposed default is “Candidate
  available”; only wording is open, not semantics.
- Whether one confirmation applies to each unqualified commit/export action or to the candidate
  digest for the current session. Inspection and running remain available without that choice.
- Default performance noise policy. Public task thresholds always take precedence; the default
  should come from `peritus-performance-qualification` evidence.

These decisions do not block S0 through S4.

## Out of scope

- Changing benchmark tasks, fixtures, hooks, resources, deadlines, verifiers, or rubrics.
- Task-ID routing, verifier inspection by the agent, expected-answer prompts, fixture-specific
  transformations, best-attempt selection, or dropping failures.
- Calling an unreviewed or gate-failing candidate accepted.
- Replacing official metrics with a cleaned score.
- Reimplementing the active command/PTY lifecycle that already exists.
- Encoding contradictory or unpublished evaluator contracts in Peritus.
- Broad redesign unrelated to an observed failure class or settlement architecture.

## Architecture verdict

**Ready for implementation after owner review.** The plan now exposes its solutions and delivery
sequence first, specifies ten complete slices with code ownership, algorithms, tests, dependencies,
and done conditions, and traces every remediable failure class to those slices. Strict acceptance
and raw benchmark accounting remain unchanged.
