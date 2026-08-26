# Feature: E3 Production Evaluation

## Summary

E3 adds `peritus-eval`, the durable and reproducible evaluation boundary between immutable E1
harness revisions and later F0 evolution. A campaign freezes the exact dataset, partition
visibility, baseline and candidate revisions, provider/model controls, execution environment,
resource policy, rollout multiplicity and seeds, evaluator artifacts, metrics, and infrastructure
failure treatment before work is admitted. It constructs a complete deterministic rollout ledger,
uses D3 for bounded work ownership, executes only committed and claimed C2/C5-facing directives,
retains every terminal observation, derives reproducible statistics, and publishes one immutable
C0 artifact/evidence bundle.

This is a production slice, not an in-memory benchmark scaffold. It includes the domain model,
canonical protocol, C0 event/checkpoint/outbox durability, replay and projection, scheduling and
cancellation, dataset isolation, rollout accounting, statistical analysis, report publication, A2
conformance, migrations, fixtures, Verus proofs, documentation, and cross-platform verification.
It does not select or promote harnesses; F0 later consumes the inert E3 report.

The design handles failures that can realistically occur here: stale or conflicting identities,
partial campaigns, worker/process/provider failure, cancellation, restart, corrupt storage,
malformed results, arithmetic bounds, and non-reproducible inputs. It does not expand E3 into
speculative distributed-consensus or unrelated release-qualification work.

## User-visible behavior

- A caller registers an immutable dataset manifest whose task and evaluator inputs are
  content-addressed artifacts partitioned as development, calibration, regression, sealed
  holdout, or canary.
- A caller freezes a complete evaluation profile. Once created, no task, harness, provider option,
  sandbox, seed, retry rule, metric, threshold, or failure treatment changes in place.
- Baseline and candidate execute the same task/rollout matrix. Stable identities and seeds derive
  from campaign inputs rather than execution order or wall time.
- D3 controls bounded admission, reservations, worker ownership, cancellation, retry, and
  backpressure. E3 owns evaluation meaning and does not reimplement scheduling.
- Every expected rollout has exactly one retained logical terminal: task pass, task failure,
  infrastructure failure, cancelled, or ambiguous. Missing results never disappear from a
  denominator and never become success.
- Infrastructure failures are always reported separately and included, excluded with the visible
  denominator, or made metric-invalid only according to the frozen policy.
- Reports include raw task/arm counts, pass@k, binomial intervals, task-cluster uncertainty,
  paired effects,
  regressions/fixes, stability, tokens, cost, latency, safety, reliability, and evidence
  completeness.
- Cancellation is durable. Work already terminal remains retained; unknown external outcomes stay
  ambiguous.
- Restart reconstructs exact state and pending outbox work without rerunning settled rollouts or
  publishing duplicate evidence.
- Default surfaces expose declared metadata and artifact identities, never sealed evaluator bytes,
  hidden tests, expected answers, credentials, or raw provider output.

## Requirements

### Dataset identity and partition isolation

- **E3-R001:** `DatasetManifest` binds a stable `DatasetId`, nonzero revision, manifest digest,
  canonical task descriptors, partition declarations, candidate-input roots, evaluator-only roots,
  verifier digests, and provenance. Its digest covers every semantic field.
- **E3-R002:** `DatasetPartition` is closed to `Development`, `Calibration`, `Regression`,
  `SealedHoldout`, and `Canary`. Task IDs are unique and ordered; each task belongs to exactly one
  partition and has nonzero weight and resource bounds.
- **E3-R003:** `CandidateTaskInput` cannot carry expected-answer or hidden-verifier roots.
  `SealedEvaluatorInput` is available only to the evaluator execution plan after candidate output
  is finalized.
- **E3-R004:** Dataset, candidate-input, evaluator, verifier, sandbox-image, and environment roots
  must be finalized C0 artifacts before campaign creation and are named as journal artifact
  dependencies.
- **E3-R005:** Production profiles reject an empty dataset, duplicate tasks, undeclared partitions,
  candidate/evaluator artifact collision, sealed content exposed as development input, missing
  digests, or configured bounds above `EvaluationLimits`.
- **E3-R006:** Dataset loading is inert and bounded. Manifests contain no repository-relative path,
  ambient discovery, mutable URL, or executable bytes.

### Frozen evaluation profile

- **E3-R010:** `EvaluationProfile` contains dataset identity/digest and selected partitions; exact
  baseline and candidate `HarnessRevisionIdentity` and governing `RevisionTuple` values; provider
  profile ID/revision/digest; model-control digest; execution/sandbox/workspace/environment/image
  digests; resource/deadline/concurrency limits; rollout count; seed and retry policies;
  evaluator/verifier digests; metric policy; and infrastructure policy.
- **E3-R011:** Baseline and candidate must differ by E1 revision digest while sharing a lineage
  unless the profile explicitly declares cross-lineage comparison. Cross-lineage results remain
  visible but are not paired F0 promotion evidence.
- **E3-R012:** `FrozenEvaluationProfile` is constructed only after exact E1 identity, finalized
  artifact, D3 capacity, required-isolation contract, and C5 profile compatibility checks.
  `ProfileDigest` is domain-separated canonical SHA-256 over every field.
- **E3-R013:** Model controls remain integer/fixed-point protocol values and bind provider/model,
  profile revision, temperature/top-p, context/output limits, tool policy,
  continuation/idempotency mode, and cost unit. E3 constructs a canonical
  `FrozenProviderSnapshot` from every public C5 profile field and an E3-owned exact retry policy;
  it never trusts a caller-supplied digest for a C5 value lacking a canonical fingerprint API.
- **E3-R014:** `RolloutSeed` is SHA-256 over a versioned domain, profile digest, task ID, and
  one-based rollout ordinal. Baseline and candidate therefore share the paired seed while their
  distinct arm-bound `RolloutId` values remain unique. Order, time, worker, and retry attempt
  cannot alter the seed. Provider delivery maps the first eight digest bytes with
  `i64::from_be_bytes`; the profile records whether C5 supports delivery or only records the seed.
  A deterministic input seed never claims deterministic provider output.
- **E3-R015:** Any dataset, harness, provider, environment, metric, threshold, retry, or seed-policy
  change creates a new profile/campaign identity and cannot accept old results.

### Deterministic rollout ledger and scheduling

- **E3-R020:** `EvaluationPlan::build` expands the frozen profile into exactly
  `tasks × selected arms × rollouts_per_task` canonical `RolloutSpec` entries after checked bounded
  multiplication. Each binds rollout/work IDs, task, partition, arm, ordinal, seed, payload,
  public candidate input, opaque evaluator schema/binding digest, execution binding, and resources.
- **E3-R020A:** Complete plan and result records use bounded content-addressed shards plus canonical
  root manifests. C0 events/checkpoints retain compact identities, shard roots, settlement status,
  and accounting summaries. `EvaluationLimits` proves the worst-case family-87 checkpoint remains
  below the 16 MiB codec ceiling; a larger corpus is represented as multiple exact campaigns rather
  than an oversized frame.
- **E3-R021:** Rollout and work IDs derive from the complete semantic identity with domain-separated
  hashes. Duplicate, missing, reordered, foreign-profile, or conflicting entries reject.
- **E3-R022:** Composite rollouts map to existing D3 `ExecutionClass::Coordination` with exact
  resources, attempts, recovery, and payload digest. D3 owns fairness, reservations, worker loss,
  and cancellation; E3 does not change D3 schema-v1 policy.
- **E3-R023:** Scheduling is commit-before-effect. A bounded queue transition stores expected
  entries and emits stable `peritus.eval.schedule-rollout.v1` C0 directives. A claimed directive is
  acknowledged only with the exact committed D3 admission or typed scheduling failure.
- **E3-R024:** Execution is commit-before-effect. D3 dispatch becomes a checked E3 directive; E3
  commits its claimed start before C2/C5 I/O and atomically records the terminal observation plus
  outbox acknowledgement.
- **E3-R025:** `RolloutExecutionPort` accepts an E3-owned inert request with exact C2/C3 isolation,
  environment, resource, deadline, teardown, and C5 profile/request bindings plus an owned
  cancellation handle. It returns one bounded E3-owned observation; it cannot mutate campaign
  state or infer promotion.
- **E3-R026:** The higher composition adapter owns C2/C3 launch and C5 execution, then maps checked
  terminal/resource/profile/request/usage observations into E3 values. `peritus-eval` validates
  every returned digest and claimed fidelity without depending on runtime/platform crates. Runtime,
  provider SDK, path, credential, and unbounded-output types stay outside its public contract.
- **E3-R027:** The candidate request contains only public task material and the opaque evaluator
  schema digest, never evaluator artifact roots. After candidate output finalization, a separate
  evaluator directive binds the hidden roots and candidate output. The two directives have
  disjoint capability views.
- **E3-R028:** Retry never changes task, arm, ordinal, seed, profile, or logical rollout identity.
  Attempts are append-only; ambiguous effects retry only when C2/C5 proves exact safety.
- **E3-R029:** Plan storage remains canonical, while dispatch order is a separate deterministic
  blocked schedule. Baseline/candidate pairs share priority; a profile/task/seed hash selects
  within-pair arm order and block order, reducing fixed temporal/provider bias reproducibly.

### Complete outcome and resource accounting

- **E3-R030:** `RolloutOutcome` is closed to `TaskPassed`, `TaskFailed`,
  `InfrastructureFailed`, `Cancelled`, and `Ambiguous`, with closed task/infrastructure classes.
- **E3-R030A:** `TaskFailed` exists only after a valid evaluator verdict says the candidate output
  is incorrect. Candidate/provider/process/evaluator execution failure is infrastructure failure;
  it cannot masquerade as evaluated incorrectness.
- **E3-R031:** A terminal record binds all campaign/profile/task/arm/seed/work/dispatch/attempt
  identities; output/evaluator digests; correctness/safety; latency; token/cost/resource
  observations; trace/evidence roots; and completeness. Missing values stay explicit.
- **E3-R031A:** D3 success means execution completed, not that the evaluated task passed. Both
  `TaskPassed` and `TaskFailed` settle D3 with the exact result digest; only execution/
  infrastructure failure uses D3 failure disposition. E3 retains the evaluation verdict.
- **E3-R032:** `RolloutLedger` admits at most one logical terminal per expected rollout. Exact
  duplicate settlement is idempotent; a conflicting settlement quarantines the campaign.
- **E3-R033:** Analysis begins only after every expected rollout is terminal or durably cancelled.
  The ledger proves `expected = passed + task_failed + infrastructure_failed + cancelled +
  ambiguous` and separately retains all attempts.
- **E3-R034:** `InfrastructurePolicy` declares per metric whether infrastructure failures count as
  failures, are excluded with denominator shown, or invalidate the metric/campaign. They are never
  silently dropped.
- **E3-R034A:** Cancelled and ambiguous rollouts always make correctness, pass@k, and paired metrics
  unavailable for the affected task/campaign. They remain separate reliability counts and are
  never configured away as task failures or ordinary exclusions.
- **E3-R035:** Cost and resources use checked integers with explicit units/fidelity. Overflow, unit
  mismatch, missing required observation, or false hard-enforcement claims are typed failures.
- **E3-R036:** Reliability separately reports task completion/failure, infrastructure rate,
  cancellation/ambiguity, retries, and observation completeness. Safety failures cannot be
  averaged away.

### Statistical analysis and stability

- **E3-R040:** Durable statistics retain raw counts and method/policy identity. Ratios use integer
  millionths with deterministic rounding; serialized native floats are never authoritative.
- **E3-R041:** pass@k uses `1 - C(n-c,k)/C(n,k)` when `n >= k`, with checked arithmetic, frozen
  infrastructure treatment, and exact zero-success and `n-c < k` behavior.
- **E3-R042:** Raw binomial correctness proportions and reliability rates may use a frozen 95%
  Wilson interval over exact numerator/denominator and retain method, confidence, bounds, and
  counts. Wilson intervals are never presented as pass@k uncertainty. Invalid denominators return
  an explicit unavailable reason.
- **E3-R043:** Paired comparison joins identical task/ordinal/seed cells and reports pass/pass,
  regressions, fixes, fail/fail, invalid pairs, and net effect. Primary uncertainty is a
  deterministic task-cluster paired bootstrap so repeated rollouts from one task are not treated as
  independent. An exact two-sided sign test is computed only from one frozen task-level sign per
  task and is reported as a diagnostic, not standalone promotion evidence.
- **E3-R044:** Bootstrap draws hash profile digest, metric ID, replicate, and draw ordinal into a
  task index, then retains that task's complete paired rollout cluster. Bounds, quantiles, sorted
  integer effects, and integer selection are frozen and portable.
- **E3-R045:** Stability retains per-task pass/fail counts, transition count, longest streaks,
  agreement millionths, and frozen-threshold classification. Prior-campaign comparison requires
  exact profile compatibility.
- **E3-R046:** Latency/cost summaries retain count, missing count, total, min, max, integer mean,
  and nearest-rank p50/p95/p99. Required missing data invalidates rather than becomes zero.
- **E3-R047:** `EvaluationReport` contains evidence and constraint violations but no promote,
  rollback, acceptance, waiver, or harness-mutation decision.

### Durable campaign, replay, cancellation, and publication

- **E3-R050:** Phases are `Created`, `Planned`, `Scheduling`, `Running`, `Cancelling`,
  `Analyzing`, `ReportReady`, `Published`, `Failed`, and `Cancelled`. The last three are terminal;
  late success cannot replace cancellation/failure.
- **E3-R051:** Commands cover campaign creation, plan/batch recording, schedule settlement, rollout
  start/attempt/terminal settlement, cancellation, analysis, publication, and explicit failure.
  Every command binds CAS history and the profile digest.
- **E3-R052:** The pure reducer rejects illegal phases, stale bindings, batch gaps, unknown
  rollouts, conflicting results, count mismatch, premature analysis/publication, and successor
  digest mismatch.
- **E3-R053:** B3 allocates inert schema-v1 families 85 `evaluation-command`, 86
  `evaluation-event`, and 87 `evaluation-state`; strict full-consumption codecs reject malformed,
  noncanonical, unknown, trailing, and oversized bytes.
- **E3-R054:** C0 allocates `AggregateKind::Evaluation` tag 15 and namespace `0xE301`. Migration v8
  expands tags 1–14 to 1–15 while preserving historical rows and bytes.
- **E3-R055:** Each transition appends one family-86 event and installs one complete compact
  family-87 checkpoint with idempotency/CAS. Plans, attempt/result bodies, raw streams, and reports
  remain bounded artifacts/shards referenced by exact roots.
- **E3-R056:** Destinations are `peritus.eval.schedule-rollout.v1`,
  `peritus.eval.execute-rollout.v1`, and `peritus.eval.publish-report.v1`. IDs derive from complete
  semantic identity; claims require exact destination/payload/identity and positive fence.
- **E3-R057:** `load_evaluation_replay` validates C0 chain, frames, campaign/revision/predecessor,
  command/event/state digests, artifact dependencies, and checkpoint. Replay must equal checkpoint.
- **E3-R058:** Recovery reconciles E3 state, D3 work, C0 outbox, finalized artifacts, and evidence,
  choosing redelivery, reconciliation, cancellation, analysis, publication retry, failure, or
  quarantine without guessing.
- **E3-R059:** `EvaluationProjection` is rebuildable and read-only, exposing phase, progress,
  counts, profile/report/evidence IDs, and recovery state without authority.
- **E3-R060:** Publication finalizes canonical report bytes in `ArtifactStore`, verifies them,
  commits the dependency, obtains integrity export, and admits idempotent `evaluation-report`
  evidence with exact provenance. Dataset/profile roots are dependencies of campaign creation;
  the report event depends only on the canonical report/root-manifest artifact cited by final
  evidence, keeping admission within C0 evidence-reference bounds.

### Errors, maintainability, formal verification, and conformance

- **E3-R070:** `EvaluationError` has closed kind/operation/recovery vocabularies covering manifest,
  profile, isolation, limits, scheduling, execution, provider, process, cancellation, statistics,
  incomplete data, transition, idempotency, journal, artifact, evidence, migration, recovery,
  codec, and corruption.
- **E3-R071:** Errors/debug output are bounded and redaction-safe: no task bodies, answers, hidden
  tests, credentials, raw output, environment values, or artifact bytes.
- **E3-R072:** Public fields stay private; roots only declare/re-export intentional modules;
  production modules target 400 and stay below 700 lines without review; forbidden generic module
  names are not used.
- **E3-R073:** No production TODO/FIXME, placeholders, unchecked success, fake production adapter,
  unsafe, ignored test, `assume`, `admit`, axiom, `external_body`, or hidden proof precondition.
- **E3-R074:** Verus covers identity validity, frozen profile projection, ledger cardinality and
  partition, unique settlement, legal transitions, terminal/cancellation dominance, attempt
  monotonicity, accounting conservation, statistical preconditions/bounds, paired conservation,
  metric bounds, and pure reducer-fold/refinement equivalence. Proofs do not claim C0 I/O, hashing,
  codec, provider, or process behavior.
- **E3-R075:** A2 conformance covers immutability, isolation, deterministic plan/seeds, bounded
  scheduling, accounting, infrastructure policy, statistics/stability, cancellation,
  replay/idempotency, malformed wire, publication, redaction, panic, and teardown.

## Acceptance criteria

| Criterion | Required evidence |
|---|---|
| Dataset fidelity | Canonical round trips and duplicate, partition, collision, missing-root, sealed-visibility, and bound rejection matrices |
| Frozen profiles | Digest vectors show every field affects identity; any profile drift rejects result admission |
| Deterministic plan | Shuffled-input/property tests reproduce complete task/arm/ordinal IDs, seeds, work payloads, and plan digest |
| Isolation | Candidate plans contain no evaluator roots; evaluation begins only after output finalization; sealed evaluator canaries never reach candidate/default output |
| Scheduling | D3 admission, dispatch, cancel, loss, retry, capacity, and backpressure retain exact E3 bindings |
| Complete accounting | Model/property traces prove one logical terminal per expected rollout and conservation across every outcome |
| Infrastructure truth | Every frozen treatment is exercised; exclusions show denominators and invalidating failures block metrics |
| Statistics | Published vectors cover pass@k, Wilson intervals, sign test, paired bootstrap, percentiles, and stability edge cases |
| Resource metrics | Token/cost/latency/C2 aggregation, missing policy, unit mismatch, fidelity, and overflow rejection |
| Durable truth | Families 85–87, tag 15, namespace `0xE301`, exact checkpoint/replay, idempotency, and claim/settlement tests |
| Crash recovery | Failpoints after plan commit, scheduling claim, D3 admission, rollout start, effect completion, result commit, analysis, artifact finalization, and evidence admission |
| Publication | Canonical report, artifact verification, journal dependency, evidence provenance, retry, and conflict tests |
| Migration | Every historical fixture migrates through v8 byte-for-byte; v7 backup/restore and tag constraints pass |
| Conformance | Nonempty A2 E3 catalog with all required scenarios, production bridge, panic containment, and teardown isolation |
| Formal quality | Strict no-cheating Verus, refinement tests, API/architecture audit, Clippy, rustdoc, focused tests, and serial Gate A |
| Hosted portability | Required Gate A and Foundation jobs pass on Ubuntu, macOS, and Windows for the exact PR head |

## Current architecture

E1 already provides checked immutable `HarnessRevisionIdentity`, protected component graphs,
content-addressed revisions, artifact roots, materialization receipts, and replay. E3 consumes
those identities and cannot create, mutate, materialize, or roll back a harness.

D3 provides `SchedulerBinding`, canonical `WorkSpec`, bounded `ResourceVector`, deterministic
selection, `SchedulerReservation`, worker ownership, cancellation, retry/loss classification,
durable transitions, and effect directives. Its payload is intentionally opaque so E3 can reuse it
without placing evaluation policy in the scheduler.

C2 provides the authorized process boundary through checked `ExecutionPlan`,
`ExecutionGateway`, owned process lifecycle, cancellation, output artifacts, terminal results, and
resource fidelity. C5 provides immutable `ProviderProfile`, normalized requests/streams, usage,
retry/idempotency, account-runtime transports, and cancellation. E3 records exact observations but
does not expose implementation handles.

C0 provides journal frames, event/checkpoint CAS, outbox claims, artifact finalization, evidence
admission, integrity export, projections, and ordered migrations. Schema v7 admits tags 1–14;
`Debugger` is tag 14. B3 ends at family 84. E3 therefore requires migration v8, aggregate tag 15,
and families 85–87.

The `analysis` layer permits foundation/state/model/orchestration/observe dependencies but not
runtime/platform crates. That is intentional for E3: `peritus-eval` owns exact inert execution
requests, observations, and validation, while later app composition binds them to C2/C3. This
satisfies the master architecture's C2 boundary without weakening the layer graph or pretending
`ExecutionGateway` can directly launch C3-restricted work.

A2 already provides runtime-neutral subject/suite patterns for scheduler, provider, process,
harness, and debugger conformance. E3 adds the analogous nonempty suite and production bridge.

## Proposed design

### Ownership and dependency flow

```text
E1 immutable harness + dataset/profile artifacts
                     |
                     v
        peritus-eval pure plan and campaign aggregate
          |              |                 |
          v              v                 v
     D3 scheduling   C2/C5 execution   C0 journal/outbox/artifacts/evidence
          \              |                 /
           \-------------v----------------/
                    immutable report
                          |
                          v
                   F0 later consumes
```

`peritus-eval` is an H package with substantial V modules. It depends on provider-neutral C5
contracts, D3, E1, and C0. It represents the required C2/C3 execution contract through E3-owned
inert values; platform backends and concrete provider adapters remain outside analysis.

### Public contract

The intentional API groups are:

- identity: `DatasetId`, `TaskId`, the existing `peritus_types::EvaluationCampaignId`,
  `EvaluationPlanId`, `RolloutId`,
  `EvaluationReportId`, `MetricId`, `ProfileDigest`, `DatasetDigest`;
- dataset/profile: `DatasetManifest`, `DatasetTask`, `DatasetPartition`, `CandidateTaskInput`,
  `SealedEvaluatorInput`, `EvaluationProfile`, `FrozenEvaluationProfile`, `EvaluationArm`,
  `SeedPolicy`, `InfrastructurePolicy`, `MetricPolicy`, `EvaluationLimits`;
- planning: `EvaluationPlan`, `RolloutSpec`, `RolloutSeed`, `SchedulingBatch`,
  `ScheduledRollout`, `build_work_spec`, `ScheduleDirectiveClaim`;
- execution/accounting: `RolloutExecutionDirective`, `RolloutExecutionPort`,
  `RawRolloutObservation`, `RolloutAttempt`, `RolloutOutcome`, `TaskFailureClass`,
  `InfrastructureFailureClass`, `RolloutLedger`, `ResourceObservation`;
- statistics/report: `PassAtK`, `WilsonInterval`, `PairedComparison`, `SignTest`,
  `BootstrapInterval`, `StabilitySummary`, `DistributionSummary`, `MetricAvailability`,
  `EvaluationAnalysis`, `EvaluationReport`, `ValidatedEvaluationReport`;
- aggregate/wire: `EvaluationCommand`, `EvaluationEvent`, `EvaluationState`,
  `EvaluationTransition`, `EvaluationCommandFrame`, `EvaluationEventFrame`,
  `EvaluationStateFrame`, `decide`, `apply_event`, `replay`;
- durability/runtime: `EvaluationReplay`, `EvaluationProjection`, `EvaluationRuntime`,
  `ExecutionDirectiveClaim`, `PublicationDirectiveClaim`, commit/load/publication/recovery APIs;
- errors: `EvaluationError`, `EvaluationErrorKind`, `EvaluationOperation`,
  `EvaluationRecovery`.

No public field exposes mutable internals. Checked constructors reject invalid state. Reports and
projections are inert and carry no capability.

The crate reuses `peritus_types::EvaluationCampaignId` and does not redefine it. It also keeps
`peritus_types` resource accounting distinct from D3 scheduler `ResourceKind`/`ResourceQuantity`;
conversions are explicit checked bindings rather than same-named aliases.

### Dataset and profile pipeline

The caller constructs bounded descriptors from finalized artifact identities. Manifest validation
does not sort silently: input must already be canonical so one semantic dataset cannot have two
encodings. `DatasetManifest::check` returns `CheckedDatasetManifest`. Profile construction then
checks selected partitions, E1 identities, execution/provider bindings, policies, and limits. Only
`FrozenEvaluationProfile` builds a plan or creates a campaign.

Sealed task metadata may reveal task ID, partition, weight, resource ceiling, and opaque digests.
It never returns task body or evaluator bytes. Artifact retrieval occurs only in the separately
authorized execution shell according to candidate/evaluator capability view.

### Planning and scheduling pipeline

`EvaluationPlan::build` iterates canonical tasks, arm order `Baseline`, `Candidate`, then one-based
rollout ordinal. It checks multiplication before allocation and derives semantic seed, rollout ID,
work ID, payload digest, and resource request for every cell. The plan digest and bounded ledger
are committed in canonical batches.

E3 uses D3 instead of an internal queue. A schedule directive contains an inert `WorkSpec` and
rollout binding. Composition claims it, submits D3, observes the committed work identity, and
settles E3 with the C0 acknowledgement. D3 dispatch translates to E3 execution only after exact
payload, worker, and dispatch agreement.

### Execution and isolation pipeline

The execution port has two ordered stages:

1. Candidate stage receives the E1 harness identity, public task input, provider/model binding,
   seed, checked C2/C3 plan and fidelity digests, and resource/deadline policy. It returns finalized
   candidate output and normalized E3-owned observations.
2. Evaluator stage receives that finalized output and sealed evaluator/verifier binding under a
   distinct C2 plan. It returns bounded correctness/safety verdict artifacts and observations.

Candidate failure prevents evaluator execution. Evaluator infrastructure failure retains the
candidate output but classifies the logical result as infrastructure failure. The port cannot
settle state; checked constructors validate its observation before a command can commit it.

### Statistical pipeline

Analysis reads only a complete checked ledger. It constructs raw arm/task and paired tables,
applies infrastructure policy without erasing raw counts, and validates denominators. It computes
pass@k, Wilson intervals for raw binomial rates, task-cluster paired bootstrap and task-level sign
diagnostics, stability, distributions, resources,
constraint violations, and completeness. Durable results use raw integers and millionths. Wider
checked temporary arithmetic is allowed; native float is never serialized or used alone at a
configured threshold.

F0 later evaluates promotion policy against these inert results and immutable inputs. E3 has no
`candidate_wins` shortcut because that would hide constraints and authority.

### Durable state and effect ordering

1. Validate finalized roots and commit campaign creation.
2. Build and commit the complete plan in bounded canonical batches.
3. Emit, claim, deliver, and settle D3 scheduling directives.
4. Claim and commit rollout start before C2/C5 effects.
5. Retain attempts and atomically settle terminal observation plus outbox acknowledgement.
6. After ledger conservation holds, commit analysis start and compute deterministic analysis.
7. Validate/finalize canonical report and commit it with its artifact dependency.
8. Claim publication, obtain integrity evidence, admit the evidence record, and atomically record
   publication plus acknowledgement.
9. Rebuild replay/projection at any point and require exact equality.

Cancellation commits first, then routes to D3 and owned C2/C5 work. Committed results remain. Late
observations may be attempt evidence but cannot change a cancelled logical terminal.

### Module layout and single-writer ownership

```text
crates/analysis/peritus-eval/
  Cargo.toml
  README.md
  src/
    lib.rs
    error.rs
    identity.rs
    limits.rs
    dataset/{mod,manifest,partition,task,validation}.rs
    profile/{mod,binding,canonical,policy}.rs
    plan/{mod,builder,rollout,seed,scheduling}.rs
    execution/{mod,directive,observation,port,adapter}.rs
    accounting/{mod,ledger,outcome,resource}.rs
    statistics/{mod,pass_at_k,interval,paired,bootstrap,stability,distribution}.rs
    report/{mod,analysis,canonical,validation}.rs
    aggregate/{mod,command,event,state,reducer}.rs
    wire/{mod,command,event,state,scalar,semantic}.rs
    durability/{mod,binding,commit,directive,replay}.rs
    runtime/{mod,driver,publication,recovery}.rs
    projection.rs
    verified.rs
  tests/
    dataset_profile.rs
    planning_scheduling.rs
    accounting_properties.rs
    statistics_vectors.rs
    execution_isolation.rs
    durability_restart.rs
    replay_wire.rs
    production_conformance.rs
    fixtures/v1/...
```

E3 has one crate in the canonical registry, so implementation remains one writer. Read-only
reviewers inspect statistical/formal and integration boundaries. The primary owns source, shared
registrations, generated fixtures, docs, Git, and verification.

### Alternatives considered

1. **In-memory benchmark loop plus JSON:** rejected because restart, denominator completeness,
   exact effect recovery, and F0 provenance would be weak.
2. **Evaluation queue inside E3:** rejected because D3 already owns bounded scheduling, fairness,
   reservations, loss, and cancellation.
3. **Aggregate scores only:** rejected because pairing, regressions, infrastructure truth,
   reproducibility, and attribution require per-rollout evidence.
4. **General statistics dependency plus serialized floats:** rejected for this frozen metric set;
   explicit integer/fixed-policy methods are smaller, auditable, portable, and verifiable.
5. **Candidate and evaluator share one input/process:** rejected because type and process separation
   is the enforceable sealed-answer boundary.
6. **E3 decides promotion:** rejected because F0 owns selection, authority, promotion, and rollback.

## Data and compatibility

Families 85–87, aggregate tag 15, namespace `0xE301`, command/event variants, identity derivation
domains, collection order, metric IDs, fixed-point scale, partition/outcome tags, and report
encoding become immutable on merge. Unknown tags and trailing bytes reject. Future optional data
requires a new schema or versioned embedded record; absence is not inferred.

Migration v8 is the established table-copy constraint expansion. It copies rows/bytes in stable
order, proves counts, updates meta/user versions transactionally, and retains backup/restore.
Downgrade after tag-15 data requires v7 backup restore or a later forward migration.

Reports retain all policy/method versions and raw inputs. They reproduce from exact event/artifact
sets. Projections remain rebuildable caches; no historical semantic data is rewritten.

## Failure handling

- Invalid manifests/profiles/plans fail before campaign creation and perform no effect.
- Stale command/event/checkpoint fences leave C0 unchanged with typed recovery guidance.
- Scheduling backpressure remains pending; explicit rejection becomes retained failure.
- Provider/process/worker failures map to closed infrastructure classes and exact digests; unknown
  outcomes remain ambiguous.
- Cancellation is terminal-dominant and idempotent.
- Missing rollouts block analysis or are explicitly cancelled; incomplete ledgers never report
  completion.
- Arithmetic overflow, invalid denominator, incompatible pairing, or missing required resource
  data returns typed unavailable/failure, never wrapped or fabricated values.
- Artifact/evidence failure retains report-ready state and resumable publication.
- Corrupt frame, chain, checkpoint, artifact, or provenance quarantines the campaign without I/O.

## Security considerations

E3 cannot grant capability, mutate a harness, accept a delivery, waive a gate, or promote a
candidate. Directives remain inert until checked by C0/D3/C2/C5 owners. Candidate/evaluator inputs
are type-separated and digest-bound. Sealed content never enters events, projections, errors, or
default reports. Credentials stay inside C5. C2 plan identity and observed fidelity prevent reports
from claiming isolation or enforcement that did not occur.

Realistic security tests cover sealed-input exposure, artifact collision, profile drift, forged
result identity, conflicting settlement, path/raw-byte diagnostics, effect-before-commit, and
authority leakage. Broader red-team/release qualification remains H0/H4.

## Verification

Heavy commands are serial with `CARGO_BUILD_JOBS=1`:

```text
cargo test --locked -p peritus-eval --all-targets --all-features
cargo test --locked -p peritus-conformance --all-targets --all-features
cargo test --locked -p peritus-journal -p peritus-migrations -p peritus-projection
cargo clippy --locked -p peritus-eval --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p peritus-eval --all-features --no-deps
cargo run --locked -p xtask -- ordinary-api-check
just verus-verify
just verus-build
just gate-a
```

Focused tests precede one complete local Gate A. Cargo, rustdoc, Verus, and Gate A never overlap.
Generated fixtures/digests are reproduced before commit. The exact PR head then passes every
required hosted Gate A and Foundation job on Ubuntu, macOS, and Windows.

Formal claims stop at represented pure properties: identity, plan determinism, transitions,
accounting, integer metric bounds/preconditions, and replay. They do not claim provider truth,
sandbox strength beyond observed fidelity, evaluator semantic correctness, or promotion authority.

## Rollout and rollback

The design lands as a signed commit before implementation. Implementation follows in signed
commits on `feature/e3-evaluation`, then a PR. E3 is not ready until local Gate A and the required
hosted matrix are green.

Before v8/tag-15 data, rollback is ordinary code rollback. Afterwards, rollback restores the v7
backup or uses a forward migration; events are never deleted or rewritten. Reports remain inert.
F0 treats unsupported E3 schemas as unavailable rather than reinterpreting them.

## Open questions

None. Scope, dependencies, tag/family allocation, isolation, scheduling ownership, durability,
statistics, fixed-point representation, formal posture, and F0 authority are resolved.

## Out of scope

- A3, G0 daemon, G1 CLI, G2 TUI, and G3 extensions.
- F0 candidate generation, attribution campaigns, selection, promotion, rollback, or production
  pointer changes.
- H0–H4 qualification beyond E3 evidence needed later.
- Harness mutation/materialization, acceptance, waivers, credential management, new sandbox
  backends, or D3 policy redesign.
- Treating significance, model self-assessment, or aggregate score as sufficient promotion proof.
