# Feature: F0 Production Harness Evolution

## Summary

F0 turns the immutable outputs of E1 harness materialization, E2 diagnosis, and E3 evaluation into
durable, evidence-backed campaign and production-pointer authorities. It owns change manifests, isolated
variants, attribution, deterministic multi-objective selection, promotion proposals, production
harness activation, and rollback. Promotion is never inferred from a score: it is a verified state
transition over exact immutable inputs and a consumed B1 human approval.

The slice adds `crates/analysis/peritus-evolution` as the canonical F0 hybrid Verus/Rust crate. Its
pure domain, validation, selection, transition, replay, and promotion predicates are executable
Verus. C0 journal, artifact, evidence, and outbox operations remain narrow ordinary-Rust effect
adapters. Campaign history and the project-scoped production pointer use distinct aggregates because
campaigns terminate while the pointer survives them. C0 commits both aggregates and B1 approve-once
consumption atomically during activation or rollback.

F0 is production implementation, not a prototype. It includes bounded domain types, complete
canonical wire formats, forward migration, replay and crash recovery, evidence publication,
rebuildable projection, A2 conformance, compatibility fixtures, formal proof roots, documentation,
and realistic contamination, metric-gaming, stale-evidence, authority, and rollback tests.

## User-visible behavior

F0 is a headless library until G0/G1/G2 expose it. Its observable behavior is:

1. A project initializes one evolution authority with an exact already-materialized E1 production
   harness and an immutable promotion-policy binding.
2. A separately authorized campaign freezes its baseline production pointer, cited E2 diagnostic
   reports, typed change manifests, isolated E1 candidate revisions, and evaluation requirements.
3. F0 admits only E3 reports whose baseline/candidate arms, dataset, profile, evaluator inputs,
   report artifact, evidence record, and campaign bindings match the declared variant exactly.
4. F0 attributes each candidate's observed task, correctness, safety, reliability, cost, latency,
   and trace effects to its declared change set. Interacting changes remain an explicit group and
   are never reported as independent effects.
5. The frozen promotion policy deterministically selects one eligible candidate or records exact
   rejection reasons. Missing or unavailable evidence fails closed.
6. Executable changes receive a completed independent D2 review bound to the exact candidate.
7. F0 constructs an exact promotion action. B1 evaluates and consumes human approval for that
   action; F0 cannot manufacture or reuse the authority.
8. Promotion atomically advances the authoritative production pointer and records the previous
   pointer. Runs already bound to an older harness remain unchanged.
9. Rollback is a newly approved activation of a known compatible earlier production revision. It
   appends history and never deletes, rewrites, or disguises the failed promotion.
10. A crash at any stage replays to the last committed state. Finalized artifacts and unacknowledged
    publication directives are reconciled idempotently.

## Requirements

- **F0-R001 — Split durable authority.** Campaigns use aggregates keyed by
  `EvolutionCampaignId`; one production-pointer aggregate keyed by `ProjectId` serializes every
  activation and rollback for that project.
- **F0-R002 — Exact initial pointer.** Initialization binds `RevisionTuple`, full E1
  `HarnessRevisionIdentity`, materialization-receipt digest, and installed snapshot identity.
- **F0-R003 — Immutable campaign identity.** A campaign has one nonzero `EvolutionCampaignId` and
  one canonical binding digest that never changes during its lifetime.
- **F0-R004 — Current-baseline fencing.** A campaign starts only when its baseline equals the exact
  current production pointer. Promotion rechecks that fence.
- **F0-R005 — Frozen policy.** Campaign and promotion decisions bind a typed schema-v1 promotion
  policy and its protected E1 `EvolutionStrategy` component digest.
- **F0-R006 — Cited diagnosis.** Every change manifest cites at least one published E2 report and
  exact E2 claim, pattern, or component-correlation evidence. Citation identity, report digest,
  artifact, evidence ID, and journal provenance are retained.
- **F0-R007 — Typed change manifest.** Each manifest records a stable ID, hypothesis and
  alternatives, exact component delta, predicted fixed and regression subjects/classes, resource
  and safety effects, falsification criteria, compatibility impact, and rollback target.
- **F0-R008 — Canonical component deltas.** Before/after component identities and digests are
  canonical, unique, and match the exact E1 baseline and candidate graphs.
- **F0-R009 — Protected-asset exclusion.** An ordinary campaign cannot alter security-root, human
  authority, sealed evaluator, trust-boundary, or production-promotion assets.
- **F0-R010 — Isolated variants.** Every variant binds one immutable materialized E1 candidate,
  one canonical nonempty change set, and a unique digest. Candidate revisions cannot equal the
  baseline or another variant.
- **F0-R011 — Interaction honesty.** A multi-change variant declares an interaction group. F0
  never emits per-change independent attribution for that group without matching isolated evidence.
- **F0-R012 — Immutable evaluation intake.** Evaluation evidence binds a validated E3 report,
  publication record, frozen profile, exact baseline/candidate arms, dataset, plan, and analysis.
- **F0-R013 — Evaluator isolation.** Candidate identities, change manifests, and evolution roles
  cannot address or replace sealed dataset/evaluator material or the promotion policy.
- **F0-R014 — Complete accounting.** Missing reports, unavailable correctness, cancellation,
  ambiguity, infrastructure-invalidated metrics, or incomplete trace/teardown observations remain
  visible and cannot silently improve eligibility.
- **F0-R015 — Deterministic attribution.** Attribution uses only E2/E3 canonical integer,
  rational, or fixed-point observations and produces byte-stable results on every supported host.
- **F0-R016 — Falsification.** Every prediction receives `Confirmed`, `Contradicted`,
  `Inconclusive`, or `NotObserved`; inconclusive evidence cannot be counted as confirmation.
- **F0-R017 — Multi-objective policy.** Selection evaluates correctness lower bound, task-level
  regressions, critical regressions, safety, stability, reliability, cost, latency, trace and
  teardown completeness, review, schema compatibility, and attribution coverage independently.
- **F0-R018 — Deny wins.** Any mandatory failed or unavailable criterion rejects the variant even
  when aggregate correctness improves.
- **F0-R019 — Stable selection.** Eligible variants are ordered by the frozen lexicographic
  objective sequence and stable variant identity; ties cannot depend on insertion or hash order.
- **F0-R020 — No self-promotion.** E2 reports, E3 reports, evolver proposals, and selection results
  are inert evidence. None independently changes the production pointer.
- **F0-R021 — Independent review.** A candidate changing executable artifacts must have a D2
  terminal `Completed` review with complete independent quorum, no unconserved findings, and exact
  candidate/revision binding. Policy may require review for additional evolvable component kinds.
- **F0-R022 — Exact promotion proposal.** The proposal digest covers project, campaign, current
  pointer, candidate pointer, change/variant/attribution/evaluation/review/policy digests, rollback
  target, and evidence-bundle artifact.
- **F0-R023 — B0/B1 authority.** Production promotion and rollback require an exact B0 dispatched
  action, durably committed B1 capability use, and a consumed B1 approve-once action whose digest,
  revision, request, decision, authority epoch/registry binding, and `HarnessPromotion` risk match
  the exact proposal.
- **F0-R024 — Approval replay safety.** One approval authorizes only its matching promotion or
  rollback action. A second use or a different action digest is rejected.
- **F0-R025 — Verified promotion safety.** Activation requires immutable inputs, an unchanged
  current baseline, complete attribution, an eligible selection, zero prohibited changes,
  compatible schema, required review, and exact authority.
- **F0-R026 — Atomic activation.** The production pointer and prior-pointer record, campaign
  terminal, both events/checkpoints, activation record, and approve-once consumption commit in one
  C0 transaction.
- **F0-R027 — Existing-run stability.** Activation does not mutate B0 run records or any existing
  run's governing `RevisionTuple`/E1 binding.
- **F0-R028 — Auditable rollback.** Rollback targets a retained previously active compatible E1
  revision, requires a new exact B1 approval, and appends a new activation record.
- **F0-R029 — Durable lifecycle.** Commands are idempotent by exact command bytes and expected
  state. Ordinary campaign/pointer commands append one semantic event and checkpoint; activation
  appends the exact campaign and pointer events/checkpoints plus approval consumption atomically.
- **F0-R030 — Replay equivalence.** Pure event fold reconstructs byte- and digest-identical state;
  terminal or superseded campaigns cannot be resurrected.
- **F0-R031 — Artifact closure.** Change manifests, attribution, selection, promotion, rollback,
  and campaign reports are finalized content-addressed artifacts and exact C0 dependencies.
- **F0-R032 — Evidence publication.** Final decisions become provenance-checked C0 evidence via a
  commit-before-effect publication outbox with exact claim acknowledgement.
- **F0-R033 — Bounded state.** Every collection, text, event, artifact set, state frame, retained
  activation, and active-campaign population has independent compiled and caller-tightenable limits.
- **F0-R034 — Closed wire schema.** Command/event/state families reject unknown schema/tag,
  malformed length, duplicate/noncanonical collections, digest disagreement, truncation, trailing
  bytes, and independent bound violations.
- **F0-R035 — Forward compatibility.** Schema migration preserves all prior journal bytes and hash
  chains and only widens the aggregate-kind registry for F0.
- **F0-R036 — Typed failures.** Errors classify invalid input, binding drift, stale state,
  contamination, policy rejection, authority denial, codec/corruption, artifact/evidence, journal,
  limit, and arithmetic failures with an actionable recovery class.
- **F0-R037 — Verus-first control.** Limits, canonical validation, attribution predicates,
  criterion evaluation, stable selection, transition legality, evaluator isolation, promotion
  safety, rollback legality, conservation, and pure replay refinement are executable Verus roots.
- **F0-R038 — No false proof claims.** Proofs do not claim SQLite, filesystem, hashing, codec I/O,
  E2/E3 truth, D2 identity, or sandbox enforcement. Those remain explicit observed/refinement
  boundaries.
- **F0-R039 — Independent conformance.** A2 owns a runtime-neutral F0 suite and the production F0
  subject passes it without sharing its implementation.
- **F0-R040 — Operational evidence.** Focused tests, full Gate A, Linux/macOS/Windows CI,
  Foundation, and Verus/no-cheating checks all pass before merge readiness.

## Acceptance criteria

1. `peritus-evolution` is registered as owner F0, layer `analysis`, verification class `H`, and has
   no forbidden dependency or source-layout exception that merely hides poor decomposition.
2. Pointer initialization, campaign freeze, evidence intake, attribution, selection, proposal,
   atomic campaign/pointer activation, a subsequent campaign, and approved rollback execute through
   public typed APIs.
3. Exact E1/E2/E3/D2/B1 bindings are checked by production constructors, not test-only assertions.
4. `INV-018 EvaluatorIsolation` and `INV-019 PromotionSafety` have executable predicates,
   specifications, proofs, ordinary-Rust refinement tests, and no-cheating verification.
5. Promotion cannot occur with a stale baseline, protected change, missing/failed/unavailable
   criterion, mismatched E3 arm, incomplete attribution, missing review, denied/stale/replayed
   approval, or mismatched action digest.
6. Promotion and rollback each commit campaign, pointer, and approve-once consumption in one
   journal append and retain the prior pointer. Restart at each adjacent crash window yields the
   last committed truth and cannot reuse the approval.
7. Campaign families 88/89/90 and production-pointer families 91/92/93 schema-v1 fixtures
   round-trip exactly; malformed and future frames fail closed.
8. Migration v9 upgrades v8 stores byte-exactly, admits aggregate kinds 16 and 17, passes integrity
   scan, and restores the verified backup.
9. A2 conformance covers the catalog and a fresh production subject, including illegal transition,
   evidence drift, selection, authority, activation, rollback, replay, and bounds cases.
10. README, CHANGELOG, `docs/f0-evolution.md`, formal proof inventory, conformance inventory,
    `architecture.toml`, manifests, checked commands, and generated protocol surfaces agree.
11. Production source contains no reachable placeholder success, `todo!`, `unimplemented!`,
    recoverable panic, unsafe code, disabled test, hidden network/process effect, or god file.
12. `CARGO_BUILD_JOBS=1 just gate-a` and every required hosted check pass on the signed PR.

### Requirement traceability

| Requirements | Primary acceptance evidence |
|---|---|
| F0-R001–F0-R005 | Public pointer/campaign lifecycle tests, exact binding tests, and replay fixtures |
| F0-R006–F0-R014 | Change/evidence constructor tests, contamination cases, and canonical wire tests |
| F0-R015–F0-R020 | Attribution/selection tests, A2 metric-gaming cases, and Verus refinement roots |
| F0-R021–F0-R028 | Review/authority tests, atomic activation crash tests, and approved rollback tests |
| F0-R029–F0-R036 | Journal/replay/publication tests, schema-v1 fixtures, migration v9, and typed error tests |
| F0-R037–F0-R040 | Formal inventory, no-cheating verification, A2 production conformance, Gate A, and hosted CI |

## Current architecture

- E1 exposes `HarnessRevision`, `HarnessRevisionIdentity`, checked component graphs,
  `GoverningHarnessBinding`, materialization receipts, immutable history, and ancestry validation.
- E2 exposes validated citation-complete reports plus durable `ReportRecord` and
  `PublicationRecord`. Reports are inert and explicitly lack mutation/promotion authority.
- E3 exposes `FrozenEvaluationProfile`, exact `HarnessArmBinding`s, `EvaluationPlan`, complete
  rollout accounting, `ValidatedEvaluationReport`, fixed-point statistics, durable publication,
  and replay. Reports are inert and explicitly defer promotion to F0.
- D2 exposes exact review candidate bindings, independent quorum, finding conservation, terminal
  state, and rebuildable projections.
- B1 already defines `OperationClass::HarnessPromotion`, `RiskClass::HarnessPromotion`, exact
  approve-once logical consumption, and C0-committed approval resolution transitions. C0 does not
  yet durably commit `ApprovalUseOutcome`; F0 must add that C0-owned adapter before activation.
- C0 supports multi-record optimistic append, complete state installation, artifact dependencies,
  transactional outbox claims/acknowledgements, evidence provenance, integrity export, and
  forward-only backed migrations.
- B3 family tags 85–87 belong to E3. Journal schema 8 admits aggregate tags 1–15. F0 therefore
  receives the next contiguous campaign and pointer allocations.
- The `analysis` layer may depend on foundation, state, model, orchestration, observe, and analysis,
  but not runtime. F0 performs no raw workspace, process, provider, or network effect.

## Proposed design

### Ownership and source layout

```text
crates/analysis/peritus-evolution/
  Cargo.toml
  README.md
  src/
    lib.rs
    identity.rs
    limits.rs
    error.rs
    binding/{mod.rs,production.rs,diagnosis.rs,evaluation.rs,review.rs,policy.rs}
    change/{mod.rs,manifest.rs,delta.rs,variant.rs,prediction.rs}
    attribution/{mod.rs,record.rs,engine.rs,falsification.rs}
    selection/{mod.rs,criterion.rs,policy.rs,decision.rs,engine.rs}
    campaign/{mod.rs,command.rs,event.rs,state.rs,reducer.rs,projection.rs}
    pointer/{mod.rs,command.rs,event.rs,state.rs,reducer.rs,projection.rs,rollback.rs}
    durability/{mod.rs,binding.rs,campaign.rs,pointer.rs,activation.rs,directive.rs,replay.rs}
    runtime/{mod.rs,artifact.rs,publication.rs,recovery.rs,authority.rs}
    wire/{mod.rs,campaign.rs,pointer.rs,semantic.rs,scalar.rs}
    verified.rs
  tests/
    fixtures/v1/{campaign-command.bin,campaign-event.bin,campaign-state.bin,
                 pointer-command.bin,pointer-event.bin,pointer-state.bin,SHA256SUMS}
    domain_campaign.rs
    attribution_selection.rs
    promotion_authority.rs
    durability_restart.rs
    publication_integration.rs
    replay_wire.rs
    production_conformance.rs
    verified_refinement.rs
```

`lib.rs` remains a small documentation/export surface. Production files target 400 lines and may
not exceed 700. A genuinely exhaustive reducer or wire mapping may receive a reviewed
`architecture.toml` exception only after it has been split by domain concern.

### Aggregate ownership and identity

F0 owns two closed C0 aggregate kinds:

- `EvolutionCampaign`, keyed by `EvolutionCampaignId`, owns one immutable campaign and terminates
  as promoted, rejected, failed, or cancelled.
- `ProductionHarness`, keyed by `ProjectId`, owns the long-lived project pointer, monotonic pointer
  generation, activation history, policy binding, and pending rollback/proposal fence.

The split keeps campaign replay and terminal semantics independent from pointer lifetime. Multiple
campaigns may evaluate concurrently, but every promotion compares and atomically advances the one
project pointer head. Two campaigns that started from the same baseline can both finish evaluation;
only the first matching promotion can win the pointer CAS. The other remains an intact reviewed
proposal and must be rejected or rebased through a successor campaign.

Campaign state contains immutable project/baseline/policy bindings, sequence/head/digest, phase,
manifests, variants, admitted E2/E3/D2 evidence, attribution, selection, proposal/publication, and a
typed terminal. Pointer state contains project, current production binding, generation,
sequence/head/digest, bounded activation history sufficient for rollback, and any pending exact
activation. Complete historical campaign and activation detail remains in semantic events and
content-addressed artifacts.

### Exact bindings

`ProductionHarnessBinding` carries the shared `RevisionTuple`, full E1
`HarnessRevisionIdentity`, materialization receipt digest, and installed snapshot digest. It is
captured from `GoverningHarnessBinding`; inert wire reconstruction is crate-private.

`DiagnosisEvidence` is captured from a completed published `DebuggerState` and matching
`ValidatedReport`. It retains job/report/manifest/query identities, report and artifact digests,
evidence ID, journal position, and the cited claim/pattern/component IDs used by a change.

`EvaluationEvidence` is captured from a completed published `EvaluationState`, matching
`ValidatedEvaluationReport`, and its `FrozenEvaluationProfile`. It verifies the report/profile
digest, baseline arm equals the campaign baseline, candidate arm equals the declared variant,
publication identity, plan, constraint observations, and full analysis digest.

These capture functions produce F0-owned canonical `PublishedDebuggerEvidence` and
`PublishedEvaluationEvidence` summaries. Public construction requires the live validated E2/E3
values and durable publication state; crate-private wire decoding exists only for replay of already
committed F0 events. The summaries retain every field used by later attribution or selection, so
restart never trusts a caller-supplied digest or depends on lossy E2/E3 projections.

`PromotionReviewEvidence` is captured from `ReviewRunState`. It requires terminal `Completed`,
complete quorum, no unconserved current findings, the exact candidate `RevisionTuple`, and D2
candidate digest equal to the full E1 candidate revision digest.

`PromotionPolicyBinding` names the protected E1 `EvolutionStrategy` declaration and binds its
component ID, content digest, owning production revision, typed policy, and policy digest. F0
rechecks that the production and campaign candidate preserve the protected declaration.

### Change manifests and variants

Change manifests are immutable and content addressed. Text fields use validated bounded UTF-8
newtypes; prediction and evidence collections are canonical, nonempty where required, and deduped
by stable identity. A `ComponentDelta` identifies one component, kind, before/after content and
optional executable digests, semantic-diff artifact, and compatibility effect.

The constructor resolves both E1 revisions and verifies every delta. Undeclared changes, omitted
changes, protected changes, equal digests, wrong component kinds, or mismatched rollback targets
are rejected. A variant contains a canonical set of manifest IDs and one candidate production
binding. A multi-manifest variant requires an `InteractionGroupId`; isolated single-manifest
variants have none.

### Attribution and falsification

The attribution engine consumes immutable manifest predictions and admitted E3 analysis. It never
reruns statistics or accepts caller-provided scores. For each prediction it records the exact metric
view, expected direction/threshold, observed integer/fixed-point value or unavailability, and one
closed verdict. Task-level pass@k/stability views and the paired transition table supply correctness
effects; E3 resource distributions and reliability counters supply operational effects.

Attribution coverage is the number of decidable declared predictions over total predictions.
Contradictions and unavailable mandatory predictions remain visible. For interaction groups, the
engine attributes the observed result to the group. Per-change attribution requires a separate E3
report for the isolated change under the same frozen evaluation basis.

### Selection and promotion policy

`PromotionPolicy` is immutable, schema-versioned, and uses only checked integers/fixed point. It
declares mandatory criteria and their thresholds, stable objective order, review-required component
kinds, maximum variants, and evidence/retention limits. The production policy always requires B1
human authority for activation and rollback.

Each `CriterionResult` is independently `Passed`, `Failed`, or `Unavailable` with exact evidence.
Eligibility requires every mandatory result to pass. The selection engine then compares eligible
variants by the frozen objective vector and finally variant ID. It returns `NoEligibleVariant`, one
`SelectedVariant`, or an explicit canonical rejection matrix; it never silently chooses a partial
result.

### Lifecycle

```text
Campaign:
Draft -> Frozen -> BaselineRunning -> Diagnosing -> Proposing -> VariantsRunning
      -> Attributing -> PromotionReview -> Promoted
                                      \-> Rejected
Any nonterminal phase -> Failed | Cancelled

Production pointer:
Uninitialized -> Active
Active -> PromotionPending -> Active(new pointer, prior retained)
Active -> RollbackPending  -> Active(previous compatible pointer, new history event)
```

The closed command vocabulary is:

- `FreezeCampaign`;
- `RecordBaselineEvidence`;
- `SubmitDiagnosis`;
- `AdmitChangeManifest`;
- `AdmitVariant`;
- `AdmitEvaluation`;
- `CompleteAttribution`;
- `RecordSelection`;
- `RequestPromotion`;
- `ActivatePromotion`;
- `RequestRollback`;
- `ActivateRollback`;
- `RecordPublication`;
- `CancelCampaign`;
- `FailCampaign`;
- pointer commands `InitializeProductionHarness`, `PreparePromotion`, `ActivatePromotion`,
  `PrepareRollback`, and `ActivateRollback`.

Every command carries its aggregate identity, expected sequence/head, prior state digest, policy
digest, exact payload, command ID, and event ID. Reducers are total: rejection returns a typed error
and no event; ordinary success returns exactly one event and successor state. Activation is the one
composite operation and requires accepted campaign and pointer transitions together.

### Promotion authority and atomic pointer change

`PromotionAction` canonical bytes are domain separated and hash every fact listed in F0-R022. The
runtime requires the matching B0 dispatched action and committed B1 capability use, then constructs
a B1 approval request with `OperationClass::HarnessPromotion` and
`RiskClass::HarnessPromotion`. `PromotionAuthorization::capture` cross-checks those receipts and the
move-only `ApprovalUseOutcome` against the proposal, current revision, authority epoch, and current
credential registry.

C0 gains `ApprovalUseCommitRequest` and `CommittedApprovalUse`. The request accepts an existing
multi-aggregate `AppendRequest`, the move-only `ApprovalUseOutcome`, expected approval-state
revision, and `CurrentCredentialRegistry`; it adds the exact consumed approval state and currentness
bindings without exposing C0's private domain-state builder. `SqliteJournal::commit_approval_use`
then commits the complete append and returns the committed approval receipt. This closes the
existing gap where approval resolution is durable but approve-once consumption is not.

`ActivatePromotion` supplies accepted campaign and pointer transitions to that adapter. One append
contains both head expectations, both semantic events, both F0 state installs, artifact
dependencies, approval-use state, and optional downstream notification. It atomically replaces the
pointer, records the previous pointer, terminalizes the campaign, and consumes approval. Retry uses
one composite digest covering every head/state/action fact; the approval cannot authorize another
proposal. The activation is authoritative journal state, not an outbox effect.

Rollback uses the same path and risk class. Its target must occur in retained activation history,
still resolve through E1, satisfy compatibility, and differ from the current pointer. The rollback
event points to the activation being reversed but does not delete it.

### Durability, artifacts, and publication

F0 receives:

- `AggregateKind::EvolutionCampaign` tag **16** and `AggregateKind::ProductionHarness` tag **17**;
- journal schema **9** and migration `v9` widening tags 1–15 to 1–17;
- campaign command/event/state families **88/89/90**, schema 1 and namespace **`0xF001`**;
- pointer command/event/state families **91/92/93**, schema 1 and namespace **`0xF002`**.

Each ordinary accepted command appends its event and installs its complete checkpoint under one
expected head/state fence. Artifact-bearing events list their exact `ArtifactDependency` set. The
request digest is based on exact command bytes. Composite activation follows the atomic path above.

Campaign decision and activation artifacts are finalized before the transition that cites them.
That transition also writes a deterministic publication outbox message. Publication verifies the
artifact, admits an `evolution-decision` or `harness-activation` `EvidenceDraft` against the exact
journal position and dependencies, and acknowledges the exact claim in the settlement transaction.
Recovery republishes an unacknowledged exact directive; duplicate admission/settlement is
idempotent. A finalized unreferenced artifact is safe garbage-collection input, not authority.

### Projection and query behavior

`EvolutionProjection` and `ProductionHarnessProjection` rebuild solely from their semantic events.
Together they expose campaign phase/evidence/selection, the current production pointer, activation
history, pending publication, and state digests. Repair discards derived projection state and
replays the journal. Projection disagreement never changes authority.

### Verus refinement boundary

`verified.rs` contains executable fact projections rather than shadow implementations. Proof roots
cover:

- legal lifecycle transitions and terminal/cancellation dominance;
- campaign input immutability and pointer baseline-currentness;
- canonical unique manifest/variant/evidence membership;
- protected-asset and evaluator/promotion-policy isolation;
- falsification and criterion classification;
- deny-wins eligibility and deterministic stable selection;
- promotion-safety conjunction and approval/action equality;
- atomic pointer successor/previous-pointer conservation;
- rollback target reachability and append-only activation history;
- pure reducer replay equivalence.

Hashing, canonical byte I/O, SQLite durability, artifact bytes, external evidence truth, and D2/B1
authentication are represented as checked facts returned by their owning boundaries and tested by
ordinary-Rust refinement/integration tests.

## Data and compatibility

F0 wire values are crate-owned and registered in B3's stable family registry. Decoders reconstruct
semantic values through production constructors and then require byte-for-byte canonical
re-encoding. Unknown versions remain storable by C0 as opaque frames but are unavailable to F0
until a compatible decoder exists.

Migration v9 rebuilds only `aggregate_heads` and `events` constraints, preserves positions, IDs,
frames, digests, hashes, indexes, metadata, and row counts, then publishes schema/user version 9.
The immutable v8 fixture proves preservation, tags 16/17 admission, and backup restore. No E1/E2/E3
stored format changes; F0 stores its restart-consumable checked evidence bridges in its own frames.

The protected proof-impact gate applies because `AggregateKind`, the B3 family registry, migration
registry, workspace package inventory, and public formal command inventory change. No existing
public constructor is weakened.

## Failure handling

Errors include operation and recovery class. Ordinary recovery choices are: correct rejected input,
retry an unchanged idempotent request, refresh stale projection/head, reconcile an outbox/artifact,
replay the aggregate, reduce scope to declared limits, request new authority, quarantine corrupt
evidence/state, or stop on an unsupported schema/integrity failure.

Likely production failures receive direct tests: stale production baseline, missing E2 citation,
mismatched E3 arm/report, unavailable correctness, safety regression, incomplete attribution,
missing executable review, denied/expired/mismatched/replayed approval, journal CAS loss, crash
before/after activation commit, publication crash windows, and incompatible rollback.

## Security considerations

- Evolution inputs are untrusted proposals until validated against immutable E1/E2/E3/D2/C0
  facts.
- Protected E1 assets, sealed evaluator material, promotion policy, and B1 authority are not
  addressable by campaign change manifests.
- A candidate correctness improvement cannot override a failed safety, review, compatibility,
  authority, or mandatory-evidence criterion.
- Human approval presentation binds the exact action digest and includes current/candidate pointer,
  campaign, policy, evidence, and rollback facts.
- F0 contains no provider, shell, filesystem, process, network, secret, or raw SQL capability.
- Default-surface errors and reports contain digests/IDs and bounded safe descriptions, never sealed
  evaluator contents, credentials, raw-vault bytes, capabilities, or approval signatures.

## Verification

Focused development checks run serially with `CARGO_BUILD_JOBS=1`:

```text
cargo fmt --all -- --check
cargo test --package peritus-evolution --all-targets --all-features --locked -- --test-threads=1
cargo clippy --package peritus-evolution --all-targets --all-features --locked -- -D warnings
cargo test --package peritus-conformance --all-targets --all-features --locked -- --test-threads=1
cargo test --package peritus-migrations --all-targets --all-features --locked -- --test-threads=1
cargo run --locked --package xtask -- architecture-check
cargo run --locked --package xtask -- ordinary-api-check
cargo verus verify --package peritus-evolution --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

Final evidence is one serialized `CARGO_BUILD_JOBS=1 just gate-a`, followed by the protected hosted
Linux/macOS/Windows Gate A and Foundation/Verus matrix. Tests use deterministic IDs, no wall-clock
ordering, no native paths/processes, no randomized map order, and no platform floating point.

The A2 suite covers initialization, exact bindings, change completeness, protected changes,
interaction groups, report/arm drift, contamination, falsification, metric gaming, unavailable
criteria, stable selection, review requirements, approval denial/replay, atomic activation,
subsequent stale campaigns, rollback, cancellation/failure, crash/replay, publication, wire
malformation, and independent bounds.

`verification/obligations.toml` currently stops at E2 despite E3's verified implementation. F0
first registers the missing E3 ledger, isolation, statistics, cancellation, replay, and non-authority
obligations against their existing proofs/tests, then adds F0 obligations for evaluator isolation,
promotion safety, deterministic attribution, deny-wins selection, approve-once atomicity, pointer
CAS, rollback history, replay, and protocol compatibility. This is inventory repair, not a claim that
new F0 proofs existed before the slice.

## Rollout and rollback

The signed design lands before implementation. Implementation lands as one F0 slice because wire,
migration, proof inventory, aggregate semantics, and fixtures must agree. The PR is not merged until
all local and hosted gates pass.

Before release, reverting F0 is an additive source revert provided no schema-9 store has been used.
After schema 9 exists, code rollback requires restoring the verified v8 backup; newer binaries may
continue reading earlier F0 frames. Product-level harness rollback always uses F0's approved
append-only activation transition, never database or Git history rewriting.

## Alternatives considered

### One aggregate and one protocol triplet

A tagged `Evolution` aggregate could hold both campaign and pointer state under families 88–90.
That saves one aggregate tag, one family triplet, and some codec code. It was rejected because
campaigns terminate while the project pointer persists, rollback is independent of campaign
lifecycle, and concurrent campaigns need one explicit project-global CAS. The split design keeps
replay, corruption handling, query ownership, and activation history clear while still committing
both transitions atomically.

### Campaign aggregate plus an external pointer effect

F0 could terminalize a campaign and emit an outbox request for G0 or E1 to change a pointer. That
was rejected because a crash would expose a promoted campaign with a stale pointer or a changed
pointer without atomic campaign truth. The authoritative pointer is F0 journal state; outbox
notifications are downstream observations only.

## Open questions

None. The split campaign/pointer aggregates, protocol allocations, durable approval-use adapter,
policy defaults, storage migration, proof boundary, and slice ownership are frozen for
implementation.

## Out of scope

- Candidate generation by a model or autonomous evolver agent; F0 accepts typed proposed changes.
- Editing/materializing harness revisions; E1 owns those effects and F0 consumes completed bindings.
- Running evaluations or recalculating E3 statistics; E3 owns those effects and facts.
- Mutating sealed datasets, evaluators, security roots, human-authority definitions, trust-boundary
  specifications, or promotion policy.
- A3 app protocol, G0 daemon composition/credential brokering, G1 CLI, G2 TUI, G3 extensions, and
  H0–H4 release qualification.
- Hosted multi-tenant evolution service or distributed consensus. Peritus remains local-first.

## Architecture verdict

**Ready.** The design assigns every durable fact to one owner, closes the existing approval-use
durability gap, preserves E1/E2/E3/D2/B1 authority boundaries, and gives implementation lanes
non-overlapping module ownership. Its proof claims stop at explicit effect boundaries, while its
acceptance evidence exercises the complete production path through atomic activation and rollback.
