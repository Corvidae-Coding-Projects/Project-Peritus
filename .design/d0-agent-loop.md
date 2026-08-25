# Feature: D0 production agent loop

## Summary

D0 introduces `peritus-agent`, the durable inner coding-agent loop that composes the already
completed lifecycle, authority, budget, persistence, workspace, tool, provider, context, memory,
and role boundaries. It is the first Peritus component that repeatedly prepares context, requests
model output, turns complete model tool calls into inert proposals, obtains independent authority,
executes tools through C4, records results, and returns them to the next model request.

The implementation is a verified deterministic reducer surrounded by narrow ordinary-Rust
adapters. The reducer never performs I/O, constructs C4 invocation permits, issues B1
capabilities, or decides acceptance. Every external effect has a durable intent/observation
boundary. A restart reconstructs the exact agent state from canonical events. An effect whose
outcome cannot be proved after restart is classified `Indeterminate` and is never silently
repeated.

D0 also supplies the small prerequisite refinements exposed by composition:

- stable B3 agent command/event/state frame families;
- a C0 `Agent` aggregate kind and forward schema migration;
- a C6 graph replacement operation that makes validated compaction operational rather than
  retaining every replaced source in selection closure; and
- durable C5 normalized-event encoding plus a provider continuation restore seam for adapters
  that can prove exact resumption.

These refinements are owned by their existing crates. They do not move lifecycle, authority,
provider, tool, context, or persistence ownership into `peritus-agent`.

## User-visible behavior

D0 is a headless library boundary. Later daemon, CLI, TUI, and delivery-orchestrator slices consume
it, but the complete loop can already be driven through its Rust API and fake production seams.

A caller can:

1. start a revision-bound writer or fixer turn;
2. retrieve scoped memory and build a role-filtered context plan;
3. negotiate a provider profile and construct an exact model request;
4. stream and durably reduce normalized provider events;
5. observe complete tool-call proposals without granting them authority;
6. independently authorize and route allowed calls through C4;
7. poll or control long-running tools while the turn remains available for pause/cancel;
8. record stable ordered results and prepare the next request;
9. receive a structured completion proposal; and
10. pause, resume, cancel, or recover the turn without inferring success.

The loop never marks a run accepted. A completion is evidence for the future E0 orchestrator, not
an acceptance decision.

## Requirements

### Ownership and composition

- **D0-R001:** `peritus-agent` shall be the sole D0 crate and shall live in
  `crates/orchestration/peritus-agent` with verification class H and verified pure modules.
- **D0-R002:** D0 shall reuse `peritus-kernel` for coarse turn/action lifecycle,
  `peritus-policy` and `peritus-budget` for authority/resource accounting, `peritus-protocol` and
  `peritus-journal` for canonical durability, C4 for all tool effects, C5 for all provider effects,
  and C6 for role/context/memory decisions.
- **D0-R003:** D0 shall not define an alternative capability, lease, approval, workspace, process,
  provider, tool, context, memory, evidence, or acceptance authority.
- **D0-R004:** The deterministic reducer shall contain no filesystem, process, network, clock,
  journal, provider, tool-router, or async handle.
- **D0-R005:** Every effect adapter shall consume a reducer-produced effect request and return a
  typed observation that must be durably accepted before it changes logical state.

### Durable turn state

- **D0-R010:** One `AgentTurnState` shall be bound to exactly one B0 `TurnId`, parent `AttemptId`,
  actor, canonical `ActorRole`/`RoleProfile`, `SessionId`, environment, `RevisionTuple`, provider
  profile identity/revision, and agent limits revision.
- **D0-R011:** The active phase vocabulary shall cover `PreparingContext`, `RequestingModel`,
  `StreamingResponse`, `ProposedToolCalls`, `AwaitingAuthorization`, `ExecutingTools`,
  `RecordingResults`, and `ProposedCompletion`, followed only by `Completed`, `Failed`, or
  `Cancelled` terminals.
- **D0-R012:** Pause shall be an explicit control state retaining the exact prior resumable phase.
  Resume shall return only to that phase after recovery preconditions are checked.
- **D0-R013:** Every state shall carry a positive logical revision, aggregate sequence, last event
  identity, and deterministic state digest.
- **D0-R014:** Commands and events shall be exhaustive closed vocabularies. Rejected commands shall
  return the unchanged state and a stable typed error.
- **D0-R015:** A successful reducer call shall return one event and one exact successor state. It
  shall not claim the event is durable.
- **D0-R016:** Replay from genesis shall reproduce the same state, phase, counters, pending effects,
  ordered tool slots, model cursor, and completion proposal as live reduction.
- **D0-R017:** Failure, cancellation, exhaustion, pause, recovery, or an indeterminate effect shall
  never produce `Completed` or a completion proposal.

### Context, role, memory, and compaction

- **D0-R020:** Context preparation shall obtain the canonical `RoleProfile` from `peritus-role` and
  use its visibility, required-content, memory, reasoning, freshness, and presentation policies.
- **D0-R021:** Memory retrieval shall use an exact scope, logical observation, role profile,
  requested features, policy, and token budget. Selected memory shall be materialized only as
  quoted, untrusted, non-authoritative `MemoryEvidence` context nodes with preserved provenance.
- **D0-R022:** Context planning shall use checked C6 node/content constructors,
  `select_context`, token accounting, and `build_render_plan`. D0 shall not concatenate hidden
  context or flatten provenance before planning.
- **D0-R023:** The D0 render adapter shall preserve one `RenderSegment` per C5 `Message`, use an
  exhaustive role mapping, reject invalid UTF-8, and visibly delimit non-authoritative evidence.
- **D0-R024:** Token estimates shall come from an injected immutable estimator profile bound to the
  selected model/profile revision. D0 shall record estimator identity and revision with the plan.
- **D0-R025:** If required context cannot fit, D0 may request a compaction proposal only for
  C6-permitted sources. It shall validate the proposal and lineage before applying it.
- **D0-R026:** C6 shall expose checked graph replacement for a `ValidatedCompaction`: replace the
  exact compacted source set, retain source-range lineage separately, preserve external
  dependencies, rewrite dependent edges deterministically, reject protected/required sources, and
  require an actual token reduction.
- **D0-R027:** Context plan, render plan, memory-selection, estimator, and compaction-policy digests
  shall be recorded before the provider request is started.

### Provider requests and streaming

- **D0-R030:** D0 shall negotiate required and optional capabilities against the exact immutable
  C5 `ProviderProfile`; unknown capability state shall never satisfy a requirement.
- **D0-R031:** Each `ModelRequest` shall bind the profile revision, negotiated capabilities,
  messages, exposed tool definitions, tool choice, bounded parallel policy, structured-output
  contract, output ceiling, cache/persistence policy, and optional continuation.
- **D0-R032:** The request fingerprint, request identity, idempotency key when legal, profile
  identity/revision, attempt number, and continuation cursor shall be durable before
  `ModelProvider::start`.
- **D0-R033:** Every normalized `EventEnvelope` shall be durably recorded in local sequence order
  before its semantic transition is externally acknowledged. Exact duplicates may be recorded as
  duplicate observations but shall not duplicate reducer effects.
- **D0-R034:** Streaming shall use C5 `ResponseReducer`; incomplete fragments, malformed content,
  missing terminal events, refusal, failure, or cancellation cannot become normal output or a tool
  proposal.
- **D0-R035:** Provider usage shall update a monotonic high-water observation and authoritative B1
  budget usage. Provider reports cannot refund or widen a budget.
- **D0-R036:** D0 model attempts count calls to `ModelProvider::start`; adapter-internal transport
  retries are not double-counted as model attempts.
- **D0-R037:** Retry decisions shall consume C5 retry legality, provider submission certainty,
  idempotency/resume protection, retry-after, accumulated attempts/retries/tokens/cost, and D0
  limits. Ambiguous unprotected submission shall stop rather than retry.
- **D0-R038:** A provider continuation may be restored after process restart only when the adapter
  accepts a persisted exact response/cursor binding through a C5-owned restore seam. Otherwise the
  response is recovered as indeterminate and is not resubmitted unsafely.
- **D0-R039:** Pause and cancel shall signal the owned live stream immediately. Confirmed provider
  cancellation is attempted only when the profile and adapter support it and a response identity
  exists. The stream remains owned until terminal or explicit indeterminate recovery.

### Tool proposals, authorization, and execution

- **D0-R040:** Only fully reduced C5 `CompletedToolCall` values may become D0 tool proposals.
  Model text, provider-native data, partial arguments, or malformed JSON shall not dispatch.
- **D0-R041:** D0 shall map model tool names and canonical arguments to C4 calls through the current
  `ExposedTools` view and `ToolRouter::prepare`. A registry-only tool hidden from the current
  exposure shall be rejected.
- **D0-R042:** Each proposal shall receive a deterministic ordinal and caller-supplied unique
  `ActionId`; it shall persist its model call identity, tool identity/version, argument digest,
  prepared digest, replay identity, revision, deadline, and declared side-effect/idempotency class.
- **D0-R043:** The reducer shall enter `AwaitingAuthorization` before any C4 dispatch. Independent
  authority assembly shall commit B0 action dispatch, capability use, budget reservation, and the
  optional mutation lease before constructing `ToolAuthorizationRequest`.
- **D0-R044:** Only C4 may construct the move-only `AuthorizedInvocation`. D0 shall never call a
  built-in dispatcher or lower workspace/process gateway directly.
- **D0-R045:** Parallel fan-out shall be the minimum of provider-negotiated parallel calls, D0
  policy, available tool-router capacity, independent authority bundles, budget capacity, and the
  hard D0 bound.
- **D0-R046:** Mutating calls shall be serialized. A successful workspace mutation invalidates the
  remainder of the old-revision batch and ends the current B0/D0 turn; the next turn starts against
  the new workspace revision.
- **D0-R047:** Inspection calls may execute concurrently only when their declared side effects and
  authority bundles are independent. Durable observations may arrive in any order, but results
  shall be presented to the model in original proposal ordinal order.
- **D0-R048:** Active calls shall be controlled only through `ToolRouter::poll`, `control`,
  `cancel`, and live-session `recover`, supporting poll, bounded stdin, PTY resize, signal, and
  cancellation exactly when the descriptor advertises each control.
- **D0-R049:** Cancellation may remain active; D0 shall continue observation until a terminal or
  indeterminate result. A crash-recovered dispatch without a restorable C4 owner shall become
  `Indeterminate` and shall never be redispatched.
- **D0-R050:** `Succeeded`, `Failed`, `Cancelled`, `TimedOut`, and `Indeterminate` tool results shall
  remain distinct. Result conversion to C5 `ToolResult` shall preserve the original call identity,
  explicit error flag, bounded model rendering, artifacts/evidence, and truncation metadata.
- **D0-R051:** Filesystem, Git, shell, and quality tools shall all be exercised through the common
  C4 loop. Shell and quality active executions shall demonstrate nonblocking poll/control behavior.

### Budgets and limits

- **D0-R060:** B1 budget accounting shall remain authoritative for model tokens, provider-cost
  microunits, active-effect milliseconds, attempts, and retries.
- **D0-R061:** D0 shall provide checked bounded counters for tool calls, provider events, context
  cycles, output bytes, tool-result bytes, concurrent calls, and total turn transitions because
  those dimensions are not represented by B1.
- **D0-R062:** All arithmetic shall be checked. Crossing any hard limit shall produce truthful
  exhaustion/failure state after cancelling owned work; it shall never clamp and continue.
- **D0-R063:** Reservation, activation, usage observation, exact settlement, cancellation, and
  ambiguous finalization shall use the B1 reducer and C0 committed receipts rather than local
  arithmetic alone.

### Completion

- **D0-R070:** A `CompletionProposal` shall contain a bounded summary, ordered fresh evidence IDs,
  unresolved uncertainties, exact workspace/specification/harness/policy/provider revision tuple,
  context/model/tool transcript digest, and a requested next phase.
- **D0-R071:** Completion may be proposed only after a normal provider terminal, no incomplete
  output items, no proposed/authorized/dispatched/active tool calls, all usage settled, and no
  cancellation, failure, exhaustion, recovery-indeterminate, or stale revision.
- **D0-R072:** Completion proposal construction shall not emit B0 acceptance commands and shall not
  claim that deterministic gates, reviews, waivers, or acceptance policy passed.
- **D0-R073:** `Completed` means only that this inner turn produced a durable valid proposal and its
  B0 turn was completed. It does not mean the attempt/run was accepted.

### Errors and operability

- **D0-R080:** Public failures shall expose a stable `AgentErrorCode`, operation, recovery class,
  turn/phase identity when known, bounded redaction-safe detail, and preserved source errors at
  ordinary-Rust boundaries.
- **D0-R081:** Recovery classes shall distinguish correct-request, retry-same-command,
  resume-provider, reconcile-tool, restart-turn, request-authority, exhausted, terminal, and
  indeterminate outcomes.
- **D0-R082:** Secret values, provider-native reasoning bytes, raw credentials, unrestricted model
  text, and raw tool output shall not appear in `Debug`, stable errors, or ordinary trace labels.
- **D0-R083:** No source file shall contain an `unsafe` block, reachable placeholder success path,
  ignored failure, detached task, unbounded channel, or unbounded retry/concurrency setting.

### Formal and maintainability requirements

- **D0-R090:** Verus shall cover phase transition legality, replay equivalence, no implicit success,
  no model-to-effect authority, bounded counters, capability/exposure gating facts, stable tool
  result ordering, and completion eligibility.
- **D0-R091:** Public structs shall keep fields private and expose checked constructors/accessors.
- **D0-R092:** The crate root shall remain below 80 lines. Ordinary source files shall target fewer
  than 400 lines and must remain below the repository hard limit. Tests shall be split by behavior.
- **D0-R093:** D0 shall add formal obligations, proof-impact registration, architecture ownership,
  ordinary-API checks, compatibility fixtures, conformance coverage, crate/root documentation, and
  a detailed changelog entry.

## Acceptance criteria

1. A scripted provider and independently authorized scripted tools complete a multi-cycle
   inspect → edit → run → test → completion flow entirely through `peritus-agent`.
2. Replaying the committed agent event chain produces byte-identical canonical state and the same
   next effect for every prefix of the end-to-end flow.
3. Starting from every nonterminal phase, pause/resume and cancel reach their documented states,
   release or retain owned work correctly, and never create completion.
4. Provider tests cover fragmented output, duplicate/out-of-order envelopes, incomplete/malformed
   tool calls, refusal, missing terminal, interruption, retry-after, protected retry, ambiguous
   unprotected submission, usage regression, live cancellation, live resume, and restart recovery.
5. Tool tests cover hidden tool names, invalid schema, authorization denial with zero effects,
   synchronous success/failure, active poll/stdin/resize/signal/cancel, deadline, parallel result
   ordering, mutation serialization/revision invalidation, router replay, and post-crash
   indeterminate no-redispatch.
6. Context tests cover role visibility, quoted memory, token-budget exhaustion, operational
   compaction replacement, render mapping, estimator binding, and iterative tool-result context.
7. Budget tests prove all B1 dimensions are reserved/observed/settled and every D0-local bound
   terminates explicitly without wraparound.
8. Completion tests reject incomplete tools, stale evidence/revisions, unsettled usage,
   indeterminate effects, failure, cancellation, and exhaustion.
9. B3 golden fixtures round-trip D0 command/event/state frames and reject truncation, unknown tags,
   invalid bounds, and trailing fields.
10. A v1 C0 database migrates forward to the Agent aggregate schema with backup/recovery evidence;
    existing records remain byte-identical and replayable.
11. Focused Rust tests, docs, Clippy, C5/C6/C4/C0 regression suites, D0 Verus no-cheating, complete
    workspace Verus, verified release builds, `just gate-a`, and hosted Linux/macOS/Windows Gate A
    and Foundation matrices all pass.

## Current architecture

### Lifecycle and authority

`peritus-kernel` owns the coarse run/attempt/turn/action lifecycle. Its turn phase is deliberately
only active/completed/failed/cancelled; D0 therefore adds inner-turn phases without changing B0.
The B0 action state already models proposed/authorized/dispatched/terminal action facts.

`peritus-policy`, `peritus-budget`, `peritus-leases`, and `peritus-approval` expose pure move-only
transitions. C0 provides typed commit adapters whose receipts are required by C4. D0 coordinates
those inputs but never weakens the router's independent cross-check.

### Persistence

`peritus-journal` accepts canonical `EventDraft` batches under aggregate-head CAS, optional durable
state installs, artifact dependencies, authority preconditions, and an outbox. `CommittedBatch` is
available only after an exact post-commit observation. Indeterminate commits are resolved using the
same command identity and request digest.

The current journal aggregate vocabulary ends at credential registries and its SQLite constraint
accepts tags one through five. D0 needs a separate tag because using `Kernel` would corrupt B0
recovery/projections and using another B1 family would violate ownership.

### Models

C5 supplies immutable profiles, capability negotiation, exact requests/fingerprints/idempotency,
owned cancellable streams, normalized event envelopes, deterministic reduction, usage tracking,
and retry legality. D0 drives these APIs and persists the facts needed to reconstruct reduction; it
does not own provider HTTP/process implementations.

### Context and memory

C6 supplies pure role profiles, deterministic memory retrieval, provenance-aware context DAGs,
selection, compaction validation, and render plans. Validated compaction currently retains source
dependencies in the selection graph, so it cannot reduce selected context. D0 requires a C6-owned
replacement operation that separates audit lineage from live selection dependencies.

### Tools

C4 owns exposure, preparation, exact authority validation, move-only invocation permits, dispatch,
active control, result normalization, and in-memory replay. D0 owns only proposal order, loop state,
and durable observations. A process restart cannot reconstruct a C4 active handle today, so the
safe D0 recovery result is indeterminate/no-redispatch unless a future C4-owned restore observation
proves a terminal result.

## Proposed design

### Crate and module layout

```text
crates/orchestration/peritus-agent/
  Cargo.toml
  README.md
  src/
    lib.rs
    command.rs
    completion.rs
    error.rs
    identity.rs
    limits.rs
    phase.rs
    effect.rs
    state/
      mod.rs
      binding.rs
      model.rs
      tools.rs
      usage.rs
    event/
      mod.rs
      context.rs
      model.rs
      tools.rs
      terminal.rs
    reducer/
      mod.rs
      validation.rs
      context.rs
      model.rs
      tools.rs
      control.rs
      terminal.rs
    codec/
      mod.rs
      command.rs
      event.rs
      state.rs
      primitive.rs
    durability/
      mod.rs
      commit.rs
      recovery.rs
      projection.rs
    context/
      mod.rs
      memory.rs
      planning.rs
      rendering.rs
      compaction.rs
      estimator.rs
    model_loop/
      mod.rs
      request.rs
      stream.rs
      retry.rs
      resume.rs
      driver.rs
    tool_loop/
      mod.rs
      proposal.rs
      authorization.rs
      execution.rs
      ordering.rs
      control.rs
    budget.rs
    driver.rs
    verified.rs
  tests/
    state_matrix.rs
    replay_matrix.rs
    context_cycle.rs
    provider_matrix.rs
    tool_matrix.rs
    control_matrix.rs
    budget_matrix.rs
    completion_matrix.rs
    production_loop.rs
    production_conformance.rs
    support/
```

The exact split may be tightened during implementation, but ownership remains as shown: pure
domain state/reduction, canonical durability, then ordinary effect adapters.

### State and reducer

`AgentTurnState` contains checked binding, phase/control state, logical revision, event head,
limits/counters, context checkpoint, model checkpoint, ordered tool batch, accumulated transcript
references, outstanding effect fence, budget references, and optional completion/failure.

The reducer API is value-in/value-out:

```text
reduce(state, command, deterministic_inputs)
    -> Result<AgentTransition, AgentRejection>
```

`AgentTransition` owns the successor and event until the durability adapter consumes it. An effect
request is derived from the committed successor, never from an uncommitted transition. Effect
identities are deterministic caller-supplied IDs bound into the command/event digest, not random
values allocated by the reducer.

The phase diagram is:

```text
PreparingContext
    -> RequestingModel
    -> StreamingResponse
    -> ProposedToolCalls
    -> AwaitingAuthorization
    -> ExecutingTools
    -> RecordingResults
    -> PreparingContext

StreamingResponse -> ProposedCompletion -> Completed

Every nonterminal phase -> Paused(previous_phase)
Every eligible paused phase -> previous_phase
Every nonterminal phase -> Cancelling -> Cancelled
Every nonterminal phase -> Failed
```

Retries remain explicit transitions back to `RequestingModel` with a durable retry plan and a
strictly increasing attempt number. Recovery is not a magic phase reset: it emits a classification
event for every outstanding effect and then resumes only along a legal normal/retry/failure path.

### Canonical protocol

B3 reserves stable families 40, 41, and 42 for agent commands, events, and durable state. DTOs use
only foundation-owned identities, revisions, bounded bytes, digests, ordinals, counters, stable
enum tags, and explicit optional values. Wire decoding reconstructs inert data only; checked D0
conversion revalidates it before reduction.

C5 adds canonical normalized `EventEnvelope` encode/decode so D0 persists exactly the semantics
already consumed by `ResponseReducer`. Provider raw wire bytes remain outside D0. C4 terminal
observations are projected into D0-owned result records containing the exact C4 canonical digest,
status, model rendering, artifacts, timing, truncation, and evidence references; D0 does not decode
or reconstruct an invocation permit.

### Durability and projection

`AggregateKind::Agent` receives stable tag 6. Its aggregate ID is derived exactly from the bound
`TurnId`. One D0 event is one aggregate append. Each append installs an agent replay capsule under
a reserved C0 state namespace. The capsule binds:

- command frame and digest;
- event frame and identity;
- exact prior head;
- deterministic reducer inputs by identity/digest;
- successor state frame/digest; and
- any model envelope/tool-result/artifact causal references.

`CommittedAgentTransition` exposes the successor only after `SqliteJournal::append` returns an
exact committed batch. Recovery loads all Agent records from genesis, decodes every event, reruns
the reducer, and compares each successor digest to its historical capsule. The latest state row is
an acceleration/checkpoint, never the source of truth.

The SQLite schema advances from version 1 to 2 to admit aggregate tag 6. A reviewed forward
migration rebuilds the two constrained aggregate tables, copies every row byte-for-byte, validates
counts/hashes/foreign keys/integrity, updates schema metadata, and requires the existing C0 backup
and recovery path. New databases install v2 directly. Compatibility tests retain an immutable v1
database fixture and verify all pre-D0 records after migration.

### Context preparation

`ContextInputs` supplies checked immutable repository/user/spec/tool transcript nodes, memory
records/index, logical observation, token estimator, provider profile, and role. D0 performs:

```text
RoleProfile
  -> memory retrieve
  -> quoted memory nodes
  -> ContextGraph
  -> select_context
  -> optional validated compaction replacement and reselection
  -> RenderPlan
  -> C5 Messages
```

The token-estimator trait is pure and returns checked estimates plus an estimator identity/revision.
Production provider adapters may supply exact tokenizer implementations later without changing the
loop contract; D0 includes a deterministic conservative estimator usable by all current providers
and records which estimator produced each plan.

Compaction is a separate provider request with a strict structured-output contract and no tools.
Its output is a `CompactionProposal`; C6 validates protected content, source ranges, lineage,
authority/trust, and token reduction. Only the C6 graph replacement operation can install the
compacted node.

### Model driver

`ModelDriver` owns one `Arc<dyn ModelProvider>` and no logical state. It starts an already committed
request, pulls one envelope at a time, and returns observations to the caller for durable reduction.
It never directly calls tools.

`AgentDriver` is cooperative rather than an uninterruptible run-to-completion future. `drive_once`
performs at most one bounded external action or one pure transition and returns control. This makes
pause, cancel, terminal input, and long-running tools naturally responsive without detached tasks.

Provider resumption uses a C5-owned `restore_continuation` method with a default explicit
unsupported result. An adapter may accept persisted response/profile/cursor bindings only when it
can re-establish the exact provider contract. D0 records the result. Unsupported or ambiguous
restoration is indeterminate, not a fresh request.

### Tool driver

`ToolBatch` stores proposal slots in original model order. A slot advances through proposed,
prepared, awaiting authority, authorized, dispatched, active, and terminal states. Each transition
retains exact identities and digests.

Authority assembly is injected through an `AgentAuthorityPort`. Its production input/output types
are existing B0/B1/C0 receipts; it cannot return a C4 permit. The tool driver passes the complete
bundle to `ToolAuthorizationRequest`, and only `ToolRouter::dispatch` may invoke a dispatcher.

The driver supports a bounded set of dispatcher registrations for filesystem, Git, shell, and
quality tools. It does not create a second registry. Active handles remain inside the driver and
are referred to from logical state by durable action/replay identities.

On live recovery the router may reconcile a retained handle. On process restart, any persisted
dispatched-but-nonterminal slot is marked indeterminate unless the C4 owner supplies an explicit
terminal recovery observation. It is never reconstructed by calling `dispatch` again.

### Budgets

`AgentBudgetPlan` maps model submissions and tool executions into exact B1 reservations. Provider
usage becomes `ObserveUsage` and exact/ambiguous terminal settlement. D0-local `AgentLimits` covers
missing structural dimensions and has a verified monotonic `AgentCounters` update.

Budget exhaustion first requests cancellation of every owned model/tool operation, then records
their terminal/indeterminate observations, settles reservations, and enters explicit failure or
cancelled state. It never proposes completion merely because the budget is exhausted.

### Completion proposal

The proposal is immutable and content-addressed. Evidence references are checked through
`peritus-evidence`; freshness must be current against the proposal revision. Summary and
uncertainties use bounded text values. The requested next phase is a closed D0 vocabulary such as
`RunGates`, `RequestReview`, `ContinueFixing`, `RequestAuthority`, or `ReportBlocked`; E0 decides
whether that request is appropriate.

### Errors

`AgentError` is the ordinary boundary error. Pure reducer rejections use a copyable stable code and
recovery class. Adapter errors preserve `source()` for local diagnostics but publish bounded
redaction-safe context. `Debug` implementations report identities, phases, sizes, and digests, not
model/tool content.

## Design alternatives

### Chosen: pure durable reducer plus cooperative effect driver

This matches B0/B1/C0 ownership, makes every phase replayable, supports deterministic fake E2E
tests, and keeps async/provider/process ownership out of Verus logic. It costs more explicit event
types but makes recovery and inspection straightforward.

### Rejected: one async `run_agent` monolith

A monolith would hide persistence cuts inside awaits, make pause/cancel race-prone, couple tests to
wall time, and encourage direct model-to-tool calls. It would also be difficult to replay or prove.

### Rejected: persist only the latest agent snapshot

A snapshot cannot prove whether a provider/tool effect was committed before a crash and does not
provide command idempotency or causal evidence. D0 uses events as truth and snapshots only as
checked acceleration.

### Rejected: reuse the B0 Kernel aggregate kind

B0 recovery and lifecycle projections assume every Kernel frame is a B0 frame. Mixing D0 events
would corrupt replay and erase ownership boundaries. A separate Agent family is required.

### Rejected: rebuild C4 replay state inside D0

C4 owns invocation permits, replay semantics, active handles, and result normalization. D0 cannot
safely reconstruct them. Honest indeterminate recovery is preferable to duplicated authority or an
unsafe redispatch.

## Data and compatibility

- Agent command/event/state frame schema starts at version 1 under stable family tags 40–42.
- `AggregateKind::Agent` has permanent tag 6 in SQLite, hash, integrity export, projection, and
  generated schema representations.
- Journal schema version 2 is forward-only. Pre-migration backup is required; rollback restores the
  verified v1 database rather than running reverse SQL.
- Existing B0/B1/B3 frame tags and bytes do not change.
- C5 normalized-event encoding is additive and versioned; existing provider request fixtures do not
  change.
- C6 compaction replacement is additive. Existing validation behavior remains accepted, while the
  new application step imposes stronger graph/token checks.
- D0 has no stable public 1.0 consumer yet, but its generated v1 compatibility corpus becomes the
  compatibility contract for later daemon/CLI releases.

## Failure handling

| Failure | Durable behavior | Recovery route |
|---|---|---|
| Context required content cannot fit | no provider effect; compaction requested or terminal context failure | adjust authorized budget/content or provide valid compaction |
| Compaction malformed/protected/non-reducing | proposal rejected; original graph retained | retry within bounded policy or fail context preparation |
| Provider start definitely not sent | request intent remains durable | legal same-request retry if budgets permit |
| Provider submission ambiguous without protection | no fresh retry | indeterminate/restart turn with human-visible cause |
| Stream duplicate | durable observation, no duplicate semantic effect | continue |
| Stream malformed/out of order/missing terminal | typed provider failure | C5 legal retry/resume or terminal failure |
| Provider cancellation unconfirmed | cancellation intent retained | observe terminal or classify indeterminate |
| Tool hidden/schema invalid | no authority/effect | return typed tool error to model or fail policy-defined turn |
| Tool authorization denied | zero dispatcher calls | return denial result or request authority |
| Tool dispatch commit indeterminate | same command identity retained | resolve C0 command before any action |
| Active tool fails | C4 normalized failure recorded | return error result to model if loop policy allows |
| Crash after dispatch, before terminal | dispatched identity retained; no redispatch | C4 recovery observation or indeterminate result |
| Mutation changes workspace revision | old turn stops scheduling calls | start a new revision-bound turn |
| Budget exhausted | owned effects cancelled/settled | explicit exhausted failure, never completion |
| Agent event append indeterminate | successor withheld | resolve exact C0 command and retry only if definitely absent |
| Replay digest mismatch | mutation mode stops | read-only diagnostics and journal repair/restore |
| Completion evidence stale | proposal rejected | gather current evidence in later D1/D2/E0 path |

## Security considerations

- Model output remains data until C4 preparation and independent committed B0/B1/C0 authority.
- C6 non-authoritative provenance and quote boundaries survive conversion into model messages.
- Provider/tool content is bounded and redacted from errors/debug output.
- No agent command can grant capability, waive findings, amend policy, or accept a run.
- Revision binding prevents an old turn from mutating or completing against a new workspace.
- Idempotency and recovery never treat uncertain external effects as safe to repeat.
- Parallel calls require independent authority and are serialized around workspace mutation.
- The implementation uses no `unsafe` and creates no detached work.

These controls address realistic agent-loop failures: stale revisions, malformed model streams,
authority bypass, duplicated effects, hidden-tool exposure, unbounded loops, and dishonest recovery.
Speculative distributed/adversarial scenarios outside a local/headless Peritus runtime are not added
to D0.

## Verification

### Focused commands

```text
CARGO_BUILD_JOBS=2 cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 cargo build --workspace --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo test --package peritus-agent --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo test --package peritus-protocol --package peritus-journal \
  --package peritus-projection --package peritus-model-protocol --package peritus-provider-core \
  --package peritus-context --package peritus-memory --package peritus-tool-router \
  --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=2 cargo doc --workspace --all-features --locked --no-deps
CARGO_BUILD_JOBS=2 cargo test --workspace --doc --all-features --locked
```

### Formal commands

```text
cargo verus verify --package peritus-agent --all-features --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo verus verify --package peritus-context --package peritus-model-protocol \
  --package peritus-journal --all-features --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

Heavyweight Verus commands run sequentially. The final authority is
`CARGO_BUILD_JOBS=2 just gate-a`, followed by the hosted Gate A and Foundation matrices on Ubuntu,
macOS, and Windows and fresh-main reruns after signed merge.

### Formal obligations

At minimum D0 registers obligations for:

1. agent phase transition legality;
2. replay/live reducer equivalence;
3. no implicit success from non-success observations;
4. model proposals confer no effect authority;
5. tool dispatch requires complete C4 authority evidence;
6. agent counters remain within configured limits;
7. terminal tool results are presented in stable proposal order;
8. completion eligibility implies no live/pending/indeterminate effect;
9. compaction replacement preserves visibility, authority, lineage, and a lower token count; and
10. Agent aggregate persistence binds the exact event and successor digest.

## Delivery and parallel work

After this design freezes public seams, implementation divides into three non-overlapping tracks:

1. **Pure agent domain:** crate scaffold, identities/limits/phases, state, commands/events, reducer,
   completion, errors, Verus specifications, and state/replay tests.
2. **Durability/protocol:** B3 agent frames, C0 Agent aggregate/schema migration, D0 commit/recovery
   adapter, projection/compatibility fixtures, and persistence tests.
3. **Runtime composition:** C6 compaction application, C5 event/resume additions, context/model/tool
   adapters, cooperative driver, fake provider/tool E2E tests, and built-in loop coverage.

Integration then adds architecture/formal/conformance registration, docs, README, changelog, full
workspace verification, QA, and delivery. Shared root files are reserved for the integration pass
to avoid agents overwriting one another.

## Rollout and rollback

D0 is merged as one production slice after candidate and hosted qualification. No feature flag or
partial runtime path is exposed. The repository remains unreleased until all later production
slices and H qualification complete.

Before any D0 Agent records exist, rollback removes the additive crate/protocol families and
restores a verified schema-v1 backup. After D0 data exists, binaries must retain schema-v2 read and
diagnostic support; rollback is a data restore, never an in-place reverse migration. Provider/tool
effects are not rolled back by deleting journal state.

## Open questions

None. The ownership and failure decisions are resolved by existing contracts:

- uncertain post-crash effects are indeterminate and never redispatched;
- workspace mutations end the old-revision turn;
- completion is a proposal only; and
- D0 remains a headless library until G0/G1.

## Out of scope

The following remain complete later slices, not omissions from D0:

- D1 gate DAG planning/execution/freshness;
- D2 review finding/quorum/reconciliation/waiver lifecycle;
- D3 multi-agent scheduling, delegation, collaboration, and fair queues;
- E0 writer → gate → reviewer → fixer delivery orchestration and run acceptance;
- C7 tracing/telemetry export and E2 failure-debugging analysis;
- daemon/IPC, CLI, TUI, MCP/plugin hosting, harness evolution, and release qualification.

D0 does integrate the current quality tool as an ordinary model-callable tool. It does not own or
interpret the later D1 acceptance gate graph.
