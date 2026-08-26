# Feature: E2 Production Debugger

## Summary

E2 adds `crates/analysis/peritus-debugger` as the Verus-first, durable diagnostic boundary for
Project Peritus. It turns immutable C7 traces plus exact E0/D0/E1/C6/C0 provenance into bounded,
evidence-linked attempt timelines, causal hypotheses, cross-run patterns, component correlations,
and harness-health summaries. It is an analysis system, not an authority system: every output is
diagnostic evidence, model output is inert until fully validated, and no E2 API can mutate a
harness, accept a run, waive a finding, execute an evaluation, or promote a candidate.

The debugger operates as typed durable jobs. A job freezes a query, the exact subject bindings,
resource limits, analysis mode, and optional model plan. It records a deterministic selection
manifest before analysis, preserves the distinction between observations and interpretations,
validates every citation against selected C7/C0 evidence, retains competing causes and contrary
evidence, persists its complete state through B3/C0, publishes the final report as a finalized C0
artifact, and admits a provenance-checked evidence record through `peritus-evidence`.

E2 is the first package whose legitimate production dependency shape crosses the existing
orchestration and observe layers: C7 supplies facts, C5/C6 supply an optional provider-neutral
model path, E0/D0 bind runs and attempts, E1 supplies immutable harness metadata, and C0 supplies
durability. The repository therefore gains a narrow `analysis` layer rather than placing a
headless domain library in `app`, weakening the `observe` layer, or granting every orchestration
crate a new dependency on observation packages.

This document freezes the complete E2 contract before implementation. There are no MVP-only or
scaffolding-only substitutions in the acceptance criteria.

## User-visible behavior

1. A caller creates one immutable `TraceSelectionQuery` over one or more exact `AnalysisSubject`
   values. Each subject binds the E0 run and attempt, current `RevisionTuple`, D0 environment and
   session, full E1 `HarnessRevisionIdentity`, and the C0 source events/digests that prove those
   bindings.
2. E2 validates each subject against recovered E0 and D0 state and the E1 projection. It rejects
   run, attempt, revision, environment, workspace, provider-profile, harness-ID, or full harness
   revision drift before selecting any trace evidence.
3. The selection engine scans a caller-supplied immutable `TraceProjectionState` and checked C0
   `IntegrityExport`, applies the query in canonical order, and writes an exact
   `TraceSelectionManifest`. The manifest records every selected trace, event, journal position,
   frame digest, causal binding, redaction/vault-reference metadata, and subject ownership. It
   contains no raw-vault bytes.
4. Deterministic analyzers normalize task outcomes separately from infrastructure outcomes,
   construct per-attempt causal timelines, classify facts using the complete initial failure
   taxonomy, generate ranked candidate causes and alternatives, retain contrary evidence and
   ambiguity, cluster failure and success patterns across subjects, and map patterns to likely E1
   component declarations and constraint levels.
5. Every report statement is explicitly an `Observation`, `Inference`, `Recommendation`, or
   `UnsupportedConclusion`. Observations require direct citations. Inferences require supporting
   citations, confidence, alternatives, and contrary evidence. Recommendations name the evidence
   and affected component classes but carry no mutation or promotion operation.
6. Citations name selected C7 event IDs and, where applicable, finalized artifact digests plus
   half-open byte ranges. Validation proves the event exists, belongs to the claimed subject and
   revision, is in the frozen manifest, and that an artifact range is nonempty and inside the
   verified finalized artifact. Citations cannot reach unselected or raw-vault content.
7. Cross-run clustering is deterministic. Identical canonical inputs and limits yield identical
   timelines, fingerprints, cluster membership, component correlations, health summaries, report
   bytes, and report digest regardless of input iteration order.
8. An optional model-assisted job uses a frozen C6 `ContextPlan`/`RenderPlan` and C5
   `ModelRequest`/`ModelProvider` stream. E2 accepts exactly one strict structured-output item,
   then reparses it into E2 types and reruns all bounds, taxonomy, provenance, citation, and report
   checks. Text-only output, tool calls, provider-native payloads, refusal, malformed streams,
   unsupported fields, bad citations, authority claims, and over-limit output are recorded as a
   typed model-analysis failure and cannot enter a report.
9. Jobs are restartable and idempotent. Selection, deterministic analysis, model request,
   model-result settlement, cancellation, report completion, artifact publication, and evidence
   admission have explicit durable states. Exact retries do not duplicate events, provider work,
   artifacts, or evidence; conflicts quarantine the job or require replay.
10. A projection exposes bounded job status, immutable query and selection digests, progress,
    budgets, retry state, report/artifact/evidence identities, and typed failures. It exposes no
    provider credentials, raw-vault bytes, capability, production pointer, evaluation result, or
    mutation handle.

## Requirements

### Identity, subjects, and frozen queries

- **E2-R001:** `DebuggerJobId`, `SelectionManifestId`, `ReportId`, `ClaimId`, `CauseId`,
  `PatternId`, and `ModelAnalysisId` are distinct nonzero 128-bit nominal types owned by
  `peritus-debugger`. They are not added to `peritus-types` because no earlier slice consumes them.
- **E2-R002:** `AnalysisSubject` is created only by a checked constructor over recovered
  `OrchestratorState`, recovered `AgentState`, `HarnessProjection`, and a selected
  `HarnessRevisionIdentity`. It binds `RunId`, `AttemptId`, `SessionId`, `EnvironmentId`,
  `RevisionTuple`, full E1 revision number/digest, E0/D0 source event IDs, source state digests, and
  their C0 journal positions. E0 and D0 revision tuples must match; D0 attempt/session must match
  the C7 scope; E1 harness ID must match the tuple and the full revision must exist.
- **E2-R003:** `TraceSelectionQuery` contains a schema version, canonical nonempty subject set,
  optional canonical trace-ID set, optional inclusive monotonic-time window, optional closed
  observation-kind and span-outcome sets, causal-closure mode, and `DebuggerLimits`. Empty
  allowlists, reversed windows, duplicate/noncanonical sets, unknown tags, and limits above
  compiled ceilings reject.
- **E2-R004:** A query is immutable and content-addressed by domain-separated SHA-256 over every
  field and complete subject binding. Caller-supplied query or manifest IDs must equal the
  canonical digest-derived identity; IDs cannot alias distinct queries.
- **E2-R005:** Causal closure is closed: `SelectedOnly` retains only events directly matching the
  filter; `IncludeAncestors` adds every selected event's transitive selected-subject C7 causal
  ancestor. No mode may add an event owned by another subject, outside the frozen C0 export, or
  beyond a compiled selection bound.
- **E2-R006:** `DebuggerLimits` bounds subjects, traces, events, causal edges, artifact citations,
  artifact bytes read, timeline entries, claims, causes per claim, contrary citations, patterns,
  pattern members, component links, report bytes, model input/output/events/tokens, attempts,
  retries, wall-time accounting, diagnostics, and complete state/event sizes. User limits may
  tighten but never widen compiled ceilings.

### Exact redacted evidence selection

- **E2-R010:** `select_evidence` accepts only a checked query, immutable C7 projection, checked C0
  integrity export, subject bindings, and an artifact verifier/reader. It performs no journal,
  trace, artifact, harness, workspace, or network mutation.
- **E2-R011:** Every selected `ProjectedObservation` must match the C0 record at its one-based
  journal position by event ID, aggregate kind `Trace`, trace aggregate ID, family 60, frame
  schema, frame digest, frame bytes, and causal parents. C0's record revision digest remains the C7
  schema digest; run revision ownership comes from the independently checked E0/D0 subject.
- **E2-R012:** Subject ownership requires the C7 `CausalBinding` session, run, and attempt to match
  the exact subject. A missing run or attempt does not silently inherit a subject; it is selectable
  only by an explicit session-only diagnostic subject mode, which is excluded from production
  report completion. Production reports require complete run and attempt bindings.
- **E2-R013:** The manifest records `RevisionTuple`, `EnvironmentId`, E1
  `HarnessRevisionIdentity`, workspace ID/generation/revision, provider profile, session/run/
  attempt, and the complete C7 causal binding for every event. Repeated subject-level data may be
  dictionary encoded on wire but must reconstruct exactly.
- **E2-R014:** Selection carries only C7 safe attributes, redaction class, omission markers, and
  finalized encrypted `ArtifactVaultReference` metadata. The default API never dereferences raw
  vault references. Ordinary artifact citations require a separately selected finalized artifact
  and do not convert a vault reference into readable content.
- **E2-R015:** Manifest entries are ordered by `(subject, journal_position, event_id)` and include
  trace/span identity, span sequence, parent span, observation time/kind, causal event IDs, frame
  digest, and frame length. The manifest stores a complete canonical digest and reports selection
  counts and limit usage without truncating silently.
- **E2-R016:** If a matching trace contains a malformed binding, missing C0 row, cross-subject
  cause, corrupt frame, or exceeded limit, selection fails as a whole. A report cannot be produced
  from a partially accepted manifest. An explicit diagnostic failure record may cite the failure
  without claiming a complete analysis.

### Outcomes, timelines, taxonomy, and root causes

- **E2-R020:** `OutcomeClass` separates `Task` and `Infrastructure`. Task outcomes include success,
  requirement failure, blocked, cancelled-by-task-policy, and indeterminate. Infrastructure
  outcomes include provider/tool/workspace/sandbox/gate-infrastructure/storage/authority/
  scheduler failures and indeterminate infrastructure. A deterministic gate assertion is a task
  failure; failure to execute or parse the gate is infrastructure.
- **E2-R021:** The closed schema-v1 `FailureCategory` catalog contains all initial architecture
  categories and subcategories:

  1. specification ambiguity, conflict, unachievable requirement;
  2. context selection, compaction, provenance;
  3. model reasoning, malformed output, refusal, completion;
  4. provider authentication, quota, rate limit, transport, protocol, accounting;
  5. tool schema, routing, authorization, execution, result normalization;
  6. workspace, patch, Git, path conflict;
  7. sandbox, process, network, resource;
  8. deterministic gate failure, gate infrastructure failure;
  9. review disagreement, invalid finding, unresolved blocker, oscillation;
  10. journal, artifact, projection, migration, recovery;
  11. approval/authority timeout, denial;
  12. scheduler starvation, cancellation, dependency failure;
  13. evolution contamination, attribution uncertainty, statistical rejection, promotion denial.

  Wire tags are explicit and append-only. `Unknown` is not a successful decode substitute; an
  unknown schema-v1 tag rejects.
- **E2-R022:** A `Timeline` is generated per complete subject from selected observations in
  `(monotonic_tick, journal_position, event_id)` order while retaining original wall-clock time.
  Each `TimelineEntry` records the source citation, normalized boundary kind, task/infrastructure
  classification if any, resource observation if present, and causal predecessor indices.
- **E2-R023:** Time regression inside a C7 span is already rejected by C7. Cross-span or cross-host
  wall-clock disagreement remains an explicit `ClockAmbiguity`; deterministic ordering falls back
  to C0 journal position and never rewrites source time.
- **E2-R024:** Deterministic analyzers use a registry of small `DeterministicAnalyzer` components.
  Initial analyzers cover terminal outcome normalization, incomplete/open spans, repeated
  diagnostics, provider/tool/gate/storage failure mapping, causal gaps, retry loops, cancellation,
  resource pressure, and success-path signatures. Analyzer order is fixed by stable analyzer tag.
- **E2-R025:** A `RootCauseCandidate` contains a stable cause ID, category, claim statement,
  supporting citations, contrary citations, alternatives, confidence in millionths, ambiguity
  flags, and derivation (`Deterministic` or validated model proposal). Confidence is bounded
  evidence strength, not calibrated probability or acceptance truth.
- **E2-R026:** A cause is valid only when support is nonempty, all citations validate, alternatives
  are distinct and canonical, contrary evidence is retained, and no statement claims certainty in
  the presence of recorded ambiguity. Unsupported conclusions are retained separately and never
  upgraded to observations or causes.

### Cross-run clustering, component mapping, and health

- **E2-R030:** `PatternFingerprint` is derived from outcome class, taxonomy category, canonical
  analyzer signature, normalized causal shape, environment class, harness revision, and component
  kind—not free-form report prose. Exact fingerprints form initial clusters; deterministic bounded
  agglomeration may combine fingerprints only under a frozen similarity policy.
- **E2-R031:** Clustering spans failure and success patterns across tasks, environments, E1 harness
  revisions, workspace revisions, provider profiles, and component classes. Every member retains
  its subject and citations; aggregate counts never replace member provenance.
- **E2-R032:** Cluster ordering is `(pattern kind, fingerprint, first subject)`; membership and
  summaries are invariant to input order. Limits produce a typed `BoundExceeded` failure rather
  than sampling or dropping members.
- **E2-R033:** `ComponentCorrelation` may reference only a component declaration present in the
  subject's exact E1 revision. It binds `ComponentId`, `ComponentKind`, content digest, protection
  class, correlation basis, supporting/contrary subject set, and `ConstraintLevel` (`Advisory`,
  `Contributing`, `Dominant`, `Unknown`). It cannot include replacement bytes, a patch, candidate
  revision, authority, evaluation, or promotion operation.
- **E2-R034:** Deterministic component mapping uses taxonomy-to-kind rules and observed C7
  tool/provider/gate bindings. It may map to likely component classes when no exact component is
  identifiable, but must mark `class_only = true` and may not fabricate an E1 component ID.
- **E2-R035:** `HarnessHealthSummary` reports bounded diagnostic counters/rates in integer
  millionths: subject coverage, successful/failed/indeterminate attempts, infrastructure share,
  repeated-pattern share, citation coverage, ambiguity share, component correlation counts, and
  per-category counts. It includes sample counts and revision scope and explicitly carries
  `DiagnosticOnly`; it has no overall pass/fail, promotion score, threshold decision, or pointer.

### Claims, citations, and report validation

- **E2-R040:** `ReportClaim` has one `ClaimKind`: `Observation`, `Inference`, `Recommendation`, or
  `UnsupportedConclusion`. The kind is part of the canonical digest and cannot be changed after
  construction.
- **E2-R041:** `EvidenceCitation` names a manifest ID, subject ID, C7 event ID, C0 journal
  position, frame digest, and optional `ArtifactCitation`. `ArtifactCitation` names a verified
  finalized ordinary artifact digest and nonempty half-open byte range. Both endpoints are u64 and
  checked against durable artifact size before validation succeeds.
- **E2-R042:** Citation validation proves: manifest identity matches; event entry exists exactly
  once; subject/run/attempt/revision/environment/harness/workspace bindings match; event/frame/
  position match; cited artifact is listed by that source event or by the report's explicitly
  selected ordinary-artifact inventory; and range is within verified bytes. Vault references are
  never accepted as ordinary artifacts by default.
- **E2-R043:** Observations require at least one validated supporting citation and no unsupported
  language marker. Inferences require support, confidence, alternatives (possibly explicit
  `NoneKnown`), and contrary-evidence inventory. Recommendations require a supported inference or
  observation parent. Unsupported conclusions may retain rejected proposal text only as a digest
  plus typed reason; they cannot contain actionable payloads.
- **E2-R044:** `DebuggerReport::validate` reruns canonical ordering, bounds, taxonomy, subject,
  timeline, cause, cluster, component, health, claim, and citation checks. Only a validated report
  can be canonically encoded, finalized as an artifact, or admitted as evidence.
- **E2-R045:** Reports never overwrite, rewrite, annotate in place, or invalidate raw C7 evidence.
  A corrected report is a new report/job/evidence identity with an explicit causal link to the
  prior evidence record.

### Optional C5/C6 model-assisted analysis

- **E2-R050:** `ModelAnalysisPlan` binds one validated deterministic draft, C6 `ContextPlanId`,
  digest of the complete `RenderPlan` segments/accounting/presentation, exact C5 provider profile
  identity/revision/model/dialect, request fingerprint, strict output-schema digest, output/event/
  token budgets, and retry policy.
- **E2-R051:** E2 maps C6 render segments to separate C5 messages without concatenating trust
  classes. System/application/user/evidence roles remain distinct. The analysis schema and frozen
  report contract are application policy; selected trace data is non-authoritative evidence.
- **E2-R052:** The C5 request contains no tools, uses `ToolChoice::None`, disallows parallel tool
  calls, requests local-first provider persistence unless the selected immutable profile and
  caller explicitly enable stored background work, and requires strict structured output against
  the checked E2 schema. The request and idempotency fingerprint are durable before provider I/O.
- **E2-R053:** `ModelAnalysisRunner` drives `ModelProvider::start`, `OwnedModelStream`, and
  `ResponseReducer`. It meters C5 usage against E2's job budget, cooperatively cancels, and records
  redaction-safe provider failures. No provider-specific SDK, executable, prompt dialect, or
  credential handling is added to E2.
- **E2-R054:** A candidate result is accepted only when C5 reaches explicit successful terminal,
  exactly one `ReducedItem::Structured` exists, no tool/provider-native/refusal/reasoning-replay
  item exists, canonical JSON is inside E2 bounds, schema-v1 decoding consumes all bytes, and the
  reconstructed proposal passes complete E2 report/citation/provenance validation.
- **E2-R055:** Model statements may add competing hypotheses, explanatory prose, or
  recommendations, but may not delete deterministic findings, hide contrary evidence, alter
  subject/selection bindings, introduce unselected citations, reclassify infrastructure as task
  success, assign acceptance, or request an effect. Any such output is rejected as a whole.
- **E2-R056:** Retry classification is explicit: safe retry is limited to C5 failures classified
  retryable before a validated result, subject to retry and budget ceilings. Invalid schema,
  invalid citations, authority language, deterministic binding disagreement, cancellation, and
  exhausted budget are terminal non-retry failures. Attempt history is retained.

### Durable jobs, cancellation, replay, and publication

- **E2-R060:** The closed job phases are `Created`, `Selected`, `DeterministicComplete`,
  `ModelPending`, `ModelRunning`, `ModelValidated`, `ReportReady`, `Published`, `Failed`, and
  `Cancelled`. `Published`, `Failed`, and `Cancelled` are terminal. Late success cannot override
  cancellation or failure.
- **E2-R061:** The command vocabulary is `CreateJob`, `RecordSelection`,
  `RecordDeterministicAnalysis`, `RequestModelAnalysis`, `MarkModelAttemptStarted`,
  `RecordModelProposal`, `RecordModelFailure`, `ScheduleModelRetry`, `CancelJob`, `CompleteReport`,
  `RecordPublication`, and `FailJob`. Each command binds expected sequence/previous event/prior
  state digest, command ID, event ID, exact job/revision/query identities, and relevant payload
  digest.
- **E2-R062:** The pure reducer rejects illegal phases, stale revision/query/manifest bindings,
  duplicate/conflicting identities, budget overrun, retry after terminal, report completion before
  deterministic analysis, model completion without a durable request/attempt, publication before
  a validated report artifact, and any event whose successor digest does not match complete state.
- **E2-R063:** B3 allocates schema-v1 inert families 82 `debugger-command`, 83 `debugger-event`, and
  84 `debugger-state`. `peritus-debugger` owns strict codecs with full-consumption decoding;
  `peritus-protocol` owns registry/generated schema/TypeScript exposure. Unknown tags, malformed,
  noncanonical, oversized, duplicate, and trailing bytes reject without state change.
- **E2-R064:** C0 allocates `AggregateKind::Debugger` tag 14 and checkpoint namespace `0xE201`.
  The aggregate ID is the exact `DebuggerJobId`. Every transition atomically appends one family-83
  event and installs one complete family-84 checkpoint with command idempotency and CAS fencing.
- **E2-R065:** Model work uses one stable C0 outbox destination
  `peritus.debugger.model-analysis.v1`; report evidence publication uses
  `peritus.debugger.publish-report.v1`. Directives are committed before effects. Claims are fenced,
  bounded, idempotent, and acknowledged in the same transaction as settlement.
- **E2-R066:** Report bytes are canonicalized and hashed before C0 commit, streamed to
  `ArtifactStore` with exact size/digest/media type/creating event, verified after finalization,
  and named as an `ArtifactDependency` in the `ReportReady`/publication transition. Existing C0
  owners perform storage; E2 does not add an artifact store.
- **E2-R067:** Evidence publication uses kind `debugger-report`, source `peritus-debugger`, the
  job's exact `RevisionTuple`, the report-commit C0 position, report payload digest/artifact, and
  explicit prior evidence causes. `EvidenceStore::admit` receives a checked journal integrity
  export and artifact store; exact retry is idempotent and conflicting identity is terminal.
- **E2-R068:** `load_debugger_replay` validates the complete C0 chain, family/schema, aggregate/job
  binding, revision digest, predecessor, command, event/state digests, artifact dependencies, and
  checkpoint head. Pure replay must equal the checkpoint. Missing/mismatched state is quarantined;
  stale but valid state requires replay, never guessed repair.
- **E2-R069:** Runtime recovery examines pending outbox directives, durable model attempt/result,
  finalized report artifact, C0 report event, and evidence catalog. It deterministically chooses
  resume, exact retry, reconcile-as-complete, cancel, fail, or quarantine. It never calls the model
  or publishes evidence without a durable directive and claim.
- **E2-R070:** `DebuggerProjection` is rebuildable solely from E2 events and exposes read-only
  status, progress, counts, digests, typed terminal/recovery state, and report/evidence references.
  It contains no command constructor or effect port.

### Errors, maintainability, and proof

- **E2-R080:** Public failures use `DebuggerError` with closed `DebuggerErrorKind`,
  `DebuggerOperation`, and `DebuggerRecovery`. Kinds distinguish invalid input, binding,
  selection, citation, taxonomy, report, model protocol, model rejection, budget, cancellation,
  illegal transition, idempotency conflict, journal, artifact, evidence, migration, recovery, and
  corruption. Display/debug output is redaction-safe and never includes model/trace/artifact bytes.
- **E2-R081:** `lib.rs` only declares modules and re-exports intentional APIs. Public fields remain
  private. Constructors validate complete invariants. Production modules are cohesive, normally
  below 400 lines and always below the 700-line hard limit without a reviewed exception. Generic
  `utils`, `helpers`, `common`, `misc`, and `manager` modules are forbidden.
- **E2-R082:** No production `TODO`, `FIXME`, `todo!`, `unimplemented!`, fake adapter, unchecked
  constructor, broad trusted wrapper, `assume`, `admit`, axiom, `external_body`, authority bypass,
  or hidden proof precondition is allowed.
- **E2-R083:** Deterministic domain/reducer/validation logic is written inside `verus!` where the
  pinned toolchain supports its dependencies. Effect adapters remain ordinary safe Rust behind
  checked plans. Executable/refinement tests connect Verus projections to production values.
- **E2-R084:** Formal obligations prove deterministic selection, citation containment,
  taxonomy/report validity, bounded analysis, reducer/replay equivalence, terminal dominance,
  report non-mutation, and absence of authority-bearing output. No proof claims provider truth,
  causal certainty, statistical validity, or raw-vault secrecy beyond the represented boundary.

## Acceptance criteria

1. `peritus-debugger` is a registered V/H package in a registered `analysis` layer with exact
   production dependencies and no forbidden reverse dependency.
2. Frozen queries and checked subjects reject every independent run, attempt, session,
   environment, revision-tuple, workspace, provider-profile, harness-ID, and full E1 revision
   mismatch.
3. Selection is deterministic under input permutation, produces the exact same canonical
   manifest, includes requested causal ancestors, excludes other subjects, validates C0 rows, and
   fails atomically on corruption or limits.
4. Redaction tests seed canaries into sensitive C7 payloads and verify that manifests, reports,
   errors, debug output, artifacts, model input fixtures, evidence bundles, and projections contain
   no raw canary. Vault references remain opaque metadata.
5. The taxonomy test enumerates every schema-v1 category/subcategory exactly once and proves
   encode/decode round trips plus unknown-tag rejection.
6. Timeline tests cover task success/failure, infrastructure failure, causal branching, open spans,
   cancellation, retry loops, clock ambiguity, deterministic ordering, and exact source citations.
7. Root-cause tests retain support, alternatives, contrary evidence, ambiguity, confidence, and
   unsupported conclusions without converting any of them into observed fact.
8. Cluster tests cover repeated failures and successes across tasks, environments, provider
   profiles, workspace revisions, harness revisions, and component classes; input permutation
   yields identical membership/fingerprints/reports.
9. E1 mapping tests use the complete 30-kind catalog, exact and class-only mappings, protected
   components, absent components, and revision drift. No output contains mutation, evaluation,
   waiver, acceptance, promotion, or production-pointer authority.
10. Citation tests reject missing events, unselected events, wrong run/attempt/revision/environment/
    harness/workspace, wrong frame/position, unrelated artifacts, out-of-range/empty ranges,
    unfinalized artifacts, vault references, duplicates, and trailing data.
11. Model tests use a deterministic fake C5 provider to exercise valid strict structured output,
    malformed stream, text output, tool call, provider-native item, refusal, invalid JSON/schema,
    invalid citation, binding drift, authority claim, retryable failure, nonretryable failure,
    cancellation, and budget exhaustion. Only the valid proposal changes durable analysis state.
12. Reducer/state-machine tests cover every legal transition and representative illegal/stale/late
    transition; terminal cancellation/failure dominates later success.
13. Durability tests prove atomic event/checkpoint/outbox writes, exact command retry, identity
    conflict, crash before/after each model and publication boundary, artifact finalization,
    evidence admission, replay equivalence, projection rebuild, and deterministic recovery.
14. B3 generated schema/TypeScript inventory includes families 82-84. Immutable E2 binary fixtures
    and SHA-256 manifest round trip exactly and reject corruption/trailing/unknown data.
15. Migration v7 upgrades immutable v6 data byte-for-byte while widening aggregate tags to 14,
    accepts a debugger aggregate afterward, survives reopen/integrity export, and restores the
    captured v6 backup exactly. Direct open of an unsupported future schema fails.
16. A2 exposes a nonempty `debugger_suite` covering selection, timeline, taxonomy, citations,
    model-output rejection, clustering, replay, cancellation, malformed inputs, redaction, bounded
    resources, panic containment, and teardown isolation. The production subject passes it.
17. Verus verifies all E2 obligations with `--no-cheating`; proof/refinement evidence is registered
    in `verification/obligations.toml` and `verification/proof-impact.toml` remains exact.
18. Focused fmt, tests, strict Clippy, rustdoc, architecture/source/API/trust/protocol/migration/
    conformance checks pass with `CARGO_BUILD_JOBS=1`.
19. README development state, `docs/e2-debugger.md`, architecture inventory, formal inventory, and
    one substantial changelog entry describe the shipped behavior and exclusions.
20. Exactly one final full local Gate A runs after focused verification. Hosted Gate A/Foundation
    are green on Linux, macOS, and Windows; signed merge reaches main; `main == origin/main`; the
    Crosslink issue closes without an auto-duplicated changelog entry.

## Current architecture

- C7 `peritus-trace` supplies `TraceProjectionState`, `TraceSnapshot`, `ProjectedObservation`,
  `Observation`, `CausalBinding`, safe attributes, redactions, and opaque
  `ArtifactVaultReference`. It deliberately exposes no raw-vault read API.
- E0 `peritus-orchestrator` durably binds a `RunId`, `AttemptId`, and exact `RevisionTuple` and can
  be replayed from C0. D0 `peritus-agent` binds attempt/session/environment/revision. Together they
  establish the subject facts C7 alone does not carry.
- E1 `peritus-harness` exposes immutable full revision identities, graphs, declarations,
  protection classes, components, and a read-only projection. It owns harness mutation and
  materialization; E2 receives no E1 command/runtime capability.
- C6 `peritus-context` exposes immutable `ContextPlanId`, `ContextPlan`, and typed `RenderPlan`
  segments retaining authority/trust/provenance. C5 exposes `ModelRequest`, strict structured
  output, `ModelProvider`, normalized owned streams, and `ResponseReducer`.
- C0 owns journal events, complete state checkpoints, outbox, artifact finalization, integrity
  export, immutable evidence admission, recovery, and migrations. The current schema is v6 and
  aggregate tags end at 13 (`Harness`).
- B3 family tags end at 81 (`harness-state`). Generated schemas and TypeScript are derived from the
  family registry; immutable family fixtures use versioned directories and digest manifests.
- A2 has runtime-neutral per-slice subject/suite contracts and already covers C7 and E1 patterns.
- Existing physical layers cannot represent E2 honestly: `observe` cannot depend on model or
  orchestration, and `orchestration` cannot depend on observe. `app` could technically depend on
  all required layers but denotes composition/UI surfaces, not this reusable headless domain.

## Proposed design

### Dependency direction and package class

Add `crates/analysis/peritus-debugger` and one `analysis` layer:

```text
foundation/state/model/orchestration/observe
                  \       |       /
                   peritus-debugger (analysis, V/H)
                              |
                       app/testing consumers
```

The layer may depend on `foundation`, `state`, `model`, `orchestration`, `observe`, and itself. It
may dev-depend on `testing`. No existing layer may depend on `analysis`; future E3/F0 may join the
layer or a later higher layer only through a reviewed architecture change. The package is H at the
Cargo boundary because it coordinates C0/C5 effects, with substantial V modules for queries,
selection, report logic, reducers, and invariants. It may depend only on provider-neutral C5
crates, never concrete provider adapters.

### Public contract

The intentional root exports are grouped as follows:

- identity/binding: `DebuggerJobId`, `SelectionManifestId`, `ReportId`, `ClaimId`, `CauseId`,
  `PatternId`, `ModelAnalysisId`, `AnalysisSubject`, `SubjectId`;
- query/selection: `TraceSelectionQuery`, `CausalClosure`, `ObservationFilter`, `DebuggerLimits`,
  `TraceSelectionManifest`, `SelectedEvidence`, `SelectedArtifact`, `select_evidence`;
- analysis: `OutcomeClass`, `TaskOutcome`, `InfrastructureOutcome`, `FailureCategory`, `Timeline`,
  `TimelineEntry`, `RootCauseCandidate`, `PatternCluster`, `PatternKind`,
  `ComponentCorrelation`, `ConstraintLevel`, `HarnessHealthSummary`, `DiagnosticStatus`;
- report: `ClaimKind`, `ReportClaim`, `EvidenceCitation`, `ArtifactCitation`, `DebuggerReport`,
  `ValidatedReport`, `validate_report`;
- model: `ModelAnalysisPlan`, `ModelProposal`, `ValidatedModelProposal`, `ModelAnalysisRunner`,
  `ModelAttemptOutcome`;
- aggregate/wire: `DebuggerCommand`, `DebuggerEvent`, `DebuggerState`, `DebuggerTransition`,
  `DebuggerCommandFrame`, `DebuggerEventFrame`, `DebuggerStateFrame`, `decide`, `replay`;
- durability/runtime: `DebuggerReplay`, `DebuggerProjection`, `DebuggerRuntime`,
  `ModelDirectiveClaim`, `PublicationDirectiveClaim`, commit/load/recovery functions;
- errors: `DebuggerError`, `DebuggerErrorKind`, `DebuggerOperation`, `DebuggerRecovery`.

No public type has public fields. No API returns mutable internal collections. Checked newtypes and
validated wrappers make invalid report/publication states unrepresentable.

### Selection and provenance pipeline

The pipeline is:

```text
E0 replay + D0 replay + E1 projection
              -> checked AnalysisSubject set
C7 projection + C0 integrity export + frozen query
              -> candidate observations
              -> subject/C0/redaction/limit/causal validation
              -> immutable TraceSelectionManifest
              -> deterministic analyzers
              -> validated deterministic report draft
```

`AnalysisSubject::from_recovered` takes read-only recovered states rather than trusting duplicated
IDs. `select_evidence` builds a C0 position index once, walks C7 traces in canonical trace order,
matches complete bindings, computes a bounded causal closure, and emits manifest entries sorted by
subject and C0 position. A `BTreeMap<EventId, ManifestEntryIndex>` provides validation lookups.
The manifest digest covers the query, subjects, entries, selected ordinary artifacts, selection
counts, and limit policy.

### Deterministic analysis pipeline

The deterministic registry emits typed facts rather than prose. Facts are merged by stable key,
then rendered into timelines and cause candidates. Exact-match fingerprints establish patterns;
the schema-v1 similarity policy may combine patterns only when category and outcome agree and the
bounded causal/component signature distance is below the frozen integer threshold. There is no
embedding or nondeterministic floating-point dependency.

Confidence uses integer millionths derived from explicit support, contradiction, ambiguity,
causal distance, and recurrence counters. The calculation saturates at declared bounds and retains
the source counters. Health uses the same integer arithmetic and never emits a pass/fail verdict.

Component mapping consumes the exact `HarnessRevision` declarations attached to each subject.
Rules map taxonomy and concrete trace bindings to `ComponentKind`; exact IDs are emitted only when
the evidence names or uniquely identifies a declaration. Ambiguous candidates are all retained in
canonical order with lower constraint strength.

### Model-assisted pipeline

The deterministic draft and selection manifest are rendered as C6 non-authoritative evidence.
The caller supplies the frozen C6 plan and immutable C5 profile/capability negotiation. E2 builds a
tool-free strict-schema `ModelRequest`, commits a model directive, claims it, and drives the
provider-neutral C5 adapter. Normalized events go through `ResponseReducer`; usage and failure
classification become durable attempt observations.

The structured result schema mirrors only proposal-capable report fields. It cannot represent a
command, patch, capability, waiver, acceptance, evaluation, promotion, or production pointer.
Decoding reconstructs E2 nominal types from strings/tags and validates every citation against the
frozen manifest. A valid proposal is merged additively with deterministic results; deterministic
facts and contrary evidence cannot be removed.

### Durable state and effect ordering

Each command runs through pure `decide`, then `commit_debugger_transition`. C0 is the only durable
authority. The runtime sequence is:

1. commit `CreateJob`;
2. compute selection read-only, commit `RecordSelection`;
3. compute deterministic analysis read-only, commit `RecordDeterministicAnalysis`;
4. if configured, commit `RequestModelAnalysis` plus outbox, claim, commit attempt-start, call C5,
   and atomically settle result/failure/retry with outbox acknowledgement;
5. validate/canonicalize the complete report, commit `CompleteReport` with report digest;
6. finalize and verify report artifact under the report event identity;
7. commit/claim publication directive, obtain integrity export, admit C0 evidence, and atomically
   record publication plus acknowledgement;
8. rebuild the projection or replay at any point to verify equivalence.

Cancellation is cooperative and durable. A cancel command first transitions the aggregate; the
runtime then cancels any owned C5 token and settles the claimed directive. A provider result that
arrives after the cancel event is retained only as a diagnostic attempt digest and cannot advance
the terminal state.

### Module layout and frozen ownership

```text
crates/analysis/peritus-debugger/
  Cargo.toml
  README.md
  src/
    lib.rs
    error.rs
    identity.rs
    limits.rs
    binding.rs
    query.rs
    selection/
      mod.rs
      engine.rs
      manifest.rs
      provenance.rs
    taxonomy.rs
    timeline/
      mod.rs
      builder.rs
    causal/
      mod.rs
      candidate.rs
      confidence.rs
    clustering/
      mod.rs
      fingerprint.rs
      engine.rs
    component/
      mod.rs
      mapping.rs
      health.rs
    citation/
      mod.rs
      artifact.rs
      validation.rs
    report/
      mod.rs
      claim.rs
      validation.rs
      canonical.rs
    model/
      mod.rs
      plan.rs
      schema.rs
      validation.rs
      runner.rs
    aggregate/
      mod.rs
      command.rs
      event.rs
      state.rs
      reducer.rs
    wire/
      mod.rs
      command.rs
      event.rs
      state.rs
      scalar.rs
    durability/
      mod.rs
      binding.rs
      commit.rs
      replay.rs
    runtime/
      mod.rs
      driver.rs
      directive.rs
      publication.rs
      recovery.rs
    projection.rs
    verified.rs
  tests/
    domain_*.rs
    model_*.rs
    durability_*.rs
    replay_wire.rs
    production_conformance.rs
    fixtures/v1/...
```

After the signed design commit, at most two implementation agents run concurrently:

- **Domain/formal lane:** owns only `src/{identity,limits,binding,query,taxonomy,verified}.rs`,
  `src/{selection,timeline,causal,clustering,component,citation,report}/**`, and
  `tests/domain_*.rs`. It does not edit manifests, wire, runtime, migrations, protocol, A2, docs,
  README, or changelog.
- **Runtime/durability lane:** owns only `src/{aggregate,wire,durability,runtime}/**`,
  `src/{model,projection}.rs` or their submodules, `tests/model_*.rs`,
  `tests/durability_*.rs`, and `tests/replay_wire.rs`. It does not edit domain-lane files or shared
  repository manifests/protocol/migrations/A2/docs.
- **Primary integration lane:** owns `Cargo.toml`, crate `Cargo.toml`, `src/lib.rs`, crate README,
  shared dependency/API adjustments, architecture policy, B3 registry/codegen, C0 aggregate/schema/
  migration/fixtures, A2 integration/conformance, verification inventories, root README/changelog,
  operator docs, integration tests, commits, PR, hosted diagnostics, and final serialized gates.

The primary creates the crate root/manifests before delegation. Agents receive explicit path
allowlists and may not run overlapping Cargo/Verus commands; the primary schedules all build/test
invocations with `CARGO_BUILD_JOBS=1`.

### Alternatives considered

1. **Put E2 in `crates/app`.** Rejected because E2 is a reusable headless domain/runtime library,
   while `app` is reserved for composition, daemon, clients, and testing surfaces. This would hide
   rather than model the new dependency boundary.
2. **Put E2 in `crates/orchestration` and allow orchestration to depend on observe.** Rejected
   because it grants every orchestration package a broad observation dependency and weakens the
   current inward dependency rule. E2 is analysis of orchestration, not orchestration authority.
3. **Put E2 in `crates/observe` and widen observe to model/orchestration.** Rejected because C7
   intentionally stays usable below the delivery loop. This would couple trace capture to model
   and harness packages.
4. **Split E2 into many crates now.** Rejected because the frozen public boundary is cohesive and
   one package with strict modules avoids premature inter-crate protocol duplication. Future E3/F0
   can consume the public API without extracting internals.
5. **Store reports only as JSON/files.** Rejected because E2 requires crash-safe jobs, replay,
   provenance, artifact roots, evidence admission, and protocol compatibility already owned by
   B3/C0.
6. **Trust model citations or model confidence.** Rejected because model output is explicitly
   untrusted proposal data; deterministic E2 validation is the production boundary.

## Data and compatibility

- Families 82/83/84 are schema-v1 and append-only. The B3 registry, generated JSON schema,
  TypeScript declarations, protocol fixture inventory, and E2 crate fixtures are updated together.
- E2 wire frames are inert. Decoding has no effect and constructors rerun semantic validation.
- `AggregateKind::Debugger = 14` is an append-only journal tag. Projection key encoding and every
  exhaustive aggregate-kind match are updated.
- Database schema v7 copies v6 aggregate heads/events into tables whose check constraint ends at
  14, checks row counts and metadata, swaps atomically, and publishes schema version 7. The v6
  fixture is immutable once committed.
- Forward upgrade requires a verified v6 backup. Before tag-14 data exists, rollback may restore
  that backup and the v6 binary. After E2 events exist, old binaries must not open the database;
  restore or a future forward migration is required.
- Canonical query, manifest, report, command, event, and state schemas carry explicit version one
  domains. Unknown future versions reject. There is no permissive JSON fallback.
- E2 reads but does not change C5/C6/C7/E0/E1 schema versions. Their exact source digests and public
  APIs are dependencies, not copied wire contracts.

## Failure handling

Failures are typed at the boundary where they occur:

- invalid subject/query/bounds: correct request, no durable analysis transition beyond failure;
- missing/mismatched C7/C0/E0/D0/E1 facts: repair dependency or quarantine;
- selection/citation/report invalidity: reject proposal and preserve original evidence;
- deterministic bound exhaustion: terminal bounded-analysis failure, never partial success;
- retryable C5/provider failure: bounded durable retry if budget remains;
- malformed or authority-bearing model output: terminal model rejection for that attempt;
- user cancellation: durable terminal cancellation and cooperative token cancellation;
- stale CAS/idempotency conflict: replay or quarantine according to exact conflict;
- artifact/evidence publication ambiguity: reconcile finalized artifact, journal dependency, and
  evidence catalog before retrying;
- corrupt chain/checkpoint/migration: quarantine and require repair/restore.

Errors never embed source bytes, provider payloads, report prose, credentials, or secrets. The
projection retains stable codes, operation, recovery, counts, and safe digests.

## Security considerations

- E2 has no capability, workspace gateway, Git/process/network/sandbox tool, E1 command, B1
  approval, E0 acceptance, E3 evaluation, F0 promotion, or production-pointer dependency.
- C5 provider I/O is available only through an already configured `ModelProvider`; E2 never loads
  credentials or chooses an executable/provider-specific transport.
- C6 authority classes remain separate. Trace/report content is evidence, never system/user
  instruction merely because it contains instruction-like text.
- C7 has already redacted default trace persistence. E2 repeats output redaction and never
  dereferences `ArtifactVaultReference` by default. Artifact citations use ordinary finalized C0
  artifacts with explicit bounded reads.
- Model output is sensitive during parsing, debug-redacted, and retained durably only after
  validation or as a digest plus safe rejection metadata.
- Report artifacts and evidence records are content-addressed and provenance-bound. They are not
  executable, are never implicitly loaded as harness components, and cannot authorize effects.
- Resource bounds cover CPU-visible collection sizes, artifact reads, stream events/output,
  retries, state, and diagnostics. The scope is precision-agent production behavior, not a claim
  of hard real-time or aircraft-control guarantees.

## Verification

Add formal obligations after the current E1 range:

- **INV-018 / E2 selection containment:** every selected entry matches exactly one frozen subject
  and C0-backed C7 event; causal closure adds only valid same-subject ancestors.
- **OBL-0168 / citation containment:** every validated citation names a selected event and any
  artifact range is nonempty and within a selected finalized artifact.
- **OBL-0169 / report validity:** every validated report is bounded, taxonomy-complete, canonically
  ordered, and every observation/inference/recommendation meets its evidence rules.
- **OBL-0170 / replay equivalence:** applying a legal event to a valid state yields the same state
  as replaying the complete canonical event prefix; terminal states cannot transition to success.
- **OBL-0171 / bounded analysis:** selection, timelines, causes, alternatives, clusters,
  correlations, claims, diagnostics, model events, and serialized state never exceed the frozen
  job limits.
- **OBL-0172 / non-mutation and non-authority:** checked reports/proposals can represent evidence,
  correlations, and recommendations but cannot represent E1 mutation, acceptance, waiver,
  evaluation, promotion, production activation, or a capability.

Evidence consists of Verus proofs in `src/verified.rs` and verified domain modules, exhaustive
ordinary/property tests, replay/fixture tests, A2 conformance, and architecture/trust scans. The
proofs use executable fact projections rather than trusted bodies. `verification/trust.toml`
remains empty unless an independently reviewed unavoidable toolchain boundary is discovered; none
is anticipated by this design.

Focused development commands are serialized and always prefixed with `CARGO_BUILD_JOBS=1`:

- package fmt/check/test/Clippy/rustdoc for `peritus-debugger`;
- no-cheating Verus verify for `peritus-debugger` and directly changed V packages;
- targeted B3 protocol generation/fixture tests;
- targeted C0 journal/projection/migration/evidence tests;
- targeted A2 debugger conformance;
- architecture, source, API, trust, proof-impact, and reproducibility checks.

After all focused checks pass, run exactly one complete local Gate A. Do not overlap Cargo, Verus,
xtask, or just processes. Hosted failures are diagnosed from exact job logs and fixed narrowly.

## Rollout and rollback

1. Commit this frozen design separately with a signed commit.
2. Create the crate root and shared registrations; delegate at most the two disjoint lanes above to
   `gpt-5.6-sol` at `xhigh`.
3. Integrate domain/runtime code, then B3/C0/A2/formal/docs in complete functional increments.
4. Generate and freeze fixtures only after schemas stabilize. Capture the v6 migration fixture and
   prove upgrade/restore before accepting tag-14 data.
5. Run focused verification, update README/changelog/docs, and run the single final local Gate A.
6. Push the signed feature branch, open the PR, monitor every hosted Gate A/Foundation job, make
   targeted fixes, and merge only when all required Linux/macOS/Windows jobs are green.
7. Verify the merge commit is signed, update local main, verify `main == origin/main`, rerun only
   any explicitly required fresh-main non-Gate-A check without duplicating the single Gate A, and
   close the Crosslink issue with `--no-changelog` after recording delivery evidence.

Rollback before tag-14 data restores the verified v6 backup and prior signed binary. After E2 data
exists, rollback is a forward operational repair or full verified backup restore; a v6 binary must
not open schema v7. Disabling optional model analysis does not require data rollback: deterministic
analysis and replay remain complete, and pending model jobs can be durably cancelled.

## Open questions

None. Layer ownership, public contracts, subject provenance, query/selection semantics, taxonomy,
citations, deterministic/model analysis, report validity, component/health limits, durable phases,
protocol families 82-84, aggregate tag 14, namespace `0xE201`, migration v7, artifact/evidence
publication, formal obligations, conformance, module boundaries, and parallel ownership are fixed.

## Out of scope

- Mutating E1 harness revisions or materialized workspaces.
- Moving or selecting production pointers.
- Running E3 evaluations or claiming statistical significance.
- Making F0 candidate, campaign, falsification, promotion, activation, or rollback decisions.
- Granting capabilities, approvals, waivers, acceptance, review disposition, or human authority.
- Reading raw-vault secret bytes by default or adding a vault decryption API.
- Provider-specific SDKs, credentials, prompts, executable routing, or transport logic.
- G0/G1/G2 daemon, CLI, TUI, remote API, dashboard, or interactive presentation behavior.
- Treating diagnostic confidence or health summaries as promotion truth.
