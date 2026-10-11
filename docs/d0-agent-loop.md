# D0 agent loop

D0 is Peritus's durable inner coding loop. It composes the existing lifecycle, authority, budget,
journal, workspace, provider, tool, role, context, memory, and evidence contracts without taking
ownership of any of them. The loop can inspect, search, edit, run, and test a revision-bound
workspace; stream a provider response; execute independently authorized tool calls; and produce a
structured completion proposal. It cannot accept a run or advance a later orchestration phase.

The implementation is split between the verified deterministic `peritus-agent` domain and narrow
ordinary adapters for persistence and effects. The reducer contains no async handles, journal
connections, provider clients, tool dispatchers, clocks, or ambient authority. The cooperative
driver performs at most one externally observable action per drive step so control operations and
durable boundaries remain explicit.

## Lifecycle

Every turn is bound to its B0 turn/attempt, actor and role, session/environment, exact workspace
and specification revisions, provider-profile revision, and agent-limit revision. Its active path
is:

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
```

Pause retains the exact resumable phase. Resume first checks that the durable binding and effect
recovery classification still match the live environment. Cancellation enters `Cancelling`, asks
owned model/tool operations to stop, and does not become `Cancelled` until their outcomes are
observed or explicitly classified. Failure and limit exhaustion are terminal facts, never
substitutes for a missing success observation.

`Completed` means only that the inner loop durably emitted an eligible completion proposal. The
future orchestrator remains responsible for review, gate evaluation, acceptance, and promotion.

## Durable command, event, and state records

B3 reserves canonical protocol families 40, 41, and 42 for inert D0 commands, events, and state
checkpoints. Each record binds the turn, command/event identity, aggregate sequence and causal
predecessor, revision tuple, phase/kind, bounded counters, successor-state digest, and a
domain-separated digest of bounded kind-specific data. Decoding one of these records never grants
authority or reconstructs an effect permit.

C0 stores D0 under the dedicated `Agent` aggregate kind. A successful transition is committed as
one journal append containing the command receipt, canonical event, and replacement state
checkpoint under aggregate-head compare-and-swap. A projection exposes the current agent head and
terminal status without becoming canonical state.

The reducer transition remains owned by the caller until the journal accepts it. Callers must not
install the successor in memory, acknowledge a provider envelope, dispatch a tool, or report a
completion before that append succeeds.

On restart, replay begins from canonical D0 events and validates every sequence, predecessor,
binding, event kind, successor digest, and counter. A saved checkpoint may accelerate replay only
after its aggregate head and state digest agree with the journal. The replayed successor must equal
the previously reduced successor.

## Context, role, and memory

Context preparation starts from `peritus-role::RoleProfile` for the canonical B1 actor role. The
caller supplies a bounded C6 context graph, explicit token estimates, retrieval candidates and
logical observation time. D0 applies memory retrieval before context selection; retrieved memory
is materialized only as quoted, non-authoritative evidence with its original provenance.

`peritus-context` performs role visibility, dependency closure, required-first selection, optional
admission, and exact token accounting. D0 records the selected plan, render digest, estimator
identity/revision, omissions, and accounting before creating a provider request. Render segments
stay individually delimited when mapped to provider-neutral C5 messages; repository, tool,
provider, model, and memory text cannot become system policy by containing instruction-like prose.

When required material cannot fit, D0 may request compaction as an explicit provider operation.
The result is not installed merely because the provider returned text. It must pass C6 lineage,
visibility, trust, protected-source, digest, and real token-reduction validation, then replace only
the selected source contribution in the next graph. Immutable policy, acceptance specifications,
active user instructions, capability facts, and unresolved blocking findings are never replaced.

## Model execution

D0 negotiates against the exact immutable C5 provider profile and records the complete semantic
request fingerprint, request identity, profile/revision, attempt number, persistence policy, and
continuation cursor before calling `ModelProvider::start`. A D0 model attempt counts this provider
start, not lower adapter transport retries.

The provider driver owns one `OwnedModelStream` and one C5 `ResponseReducer`. It pulls at most one
normalized envelope, retains it as pending, and returns it for durable recording. Only after the D0
append succeeds may the envelope enter the response reducer and the next envelope be pulled. This
preserves event ordering and prevents an acknowledged stream position from outrunning durable
state.

Normalized envelopes have a stable bounded codec and are retained in their durable D0 provider
event records. On exact resume, D0 decodes the committed prefix and rebuilds the C5 response
reducer before it asks the adapter to restore the profile-bound continuation. The restored
continuation must exactly match the recorded provider response identity and cursor; unsupported or
mismatched restoration never starts a replacement request. If a request was possibly submitted
but there is neither exact idempotency nor an exact continuation, restart classifies the outcome
as indeterminate instead of silently issuing another request.

The C5 reducer remains authoritative for duplicate provider events, local/provider ordering,
fragmented UTF-8 and JSON, complete tool calls, usage high-water accounting, refusals, incomplete
responses, cancellation, and terminal classification. D0 accepts only complete reduced tool calls
as inert proposals. Provider-native reasoning replay bytes are bounded protocol data and are never
included in diagnostics.

## Tool authorization and execution

Model-produced tool calls have no effect authority. D0 resolves the name only through the current
C4 `ExposedTools` view and asks `ToolRouter::prepare` to validate the descriptor, version, schema,
arguments, limits, and replay identity. Unknown, hidden, or malformed calls fail without invoking a
dispatcher.

Each prepared call receives a caller-supplied action identity and records its proposal ordinal,
model call identity, tool identity/version, argument and descriptor digests, side-effect class,
deadline, output limit, and idempotency class. Dispatch requires the complete C4 authorization
request: committed B0 action state, capability and scope, B1 budget receipt, optional lease, current
epoch, exact revision binding, and logical observation time. D0 cannot mint or infer any part of
that bundle.

Read-only calls may run in bounded parallel when the model profile, tool descriptor, policy,
router capacity, budget, and local fan-out bound all permit it. Mutating calls are serialized. A
successful mutation changes the workspace revision and ends the old revision-bound turn; later
work starts under a fresh turn rather than carrying stale authority forward.

Long-running invocations remain owned by C4. One cooperative step may poll, send bounded stdin,
resize a PTY, send a supported signal, request cancellation, or observe terminal state. Completion,
failure, cancellation, timeout, and indeterminate loss are all explicit terminal tool results.
Results may complete in any physical order but are rendered back to the model in original proposal
ordinal order.

After a process crash, an invocation that was durably dispatched but lacks a terminal observation
is never dispatched again. If C4 cannot re-establish the exact active handle, D0 records an
`Indeterminate` result and exposes reconciliation as the recovery action. This is intentionally
honest about the effect boundary.

## Budgets and limits

B1 remains authoritative for model tokens, provider cost, active-effect time, attempts, and
retries. D0 additionally checks structural bounds for provider events, model output bytes, tool
calls, tool-result bytes, context cycles, concurrent calls, and total reducer transitions. Every
increment uses checked arithmetic and is committed with the event that caused it.

Before a model start or tool dispatch, the runtime builds a checked `AgentBudgetPlan` and requires
a matching held B1 reservation. It activates that reservation only after the effect starts. C5
usage high-water observations update the active reservation without treating them as final;
terminal provider usage reconciles the exact token, cost, and active-time amounts. Attempts are
charged once per provider start and retries only for retry starts. Missing or ambiguous terminal
usage consumes the reserved ceiling through B1's indeterminate settlement instead of inventing a
lower total.

Exhaustion stops new work, requests cancellation of owned operations, records the exact exhausted
dimension, and reaches a typed terminal outcome after reconciliation. No exhausted counter wraps,
and no soft provider observation silently increases an authoritative budget.

## Completion proposals

A completion proposal contains a bounded summary, canonical evidence references, unresolved
uncertainties, the exact workspace/specification revision tuple, context/model/tool transcript
digests, and the requested next orchestration phase. Eligibility requires a successful provider
terminal, no open output items, no proposed/authorized/dispatched/active tool calls, settled usage,
fresh evidence, and exact revision agreement.

Producing the proposal and completing the D0 turn are separate durable transitions. Neither one
accepts a candidate, waives a finding, marks a gate green, or promotes a workspace.

## Recovery classifications

Operational errors use stable typed codes and recovery classes. The important operator actions are:

- retry only when the recorded provider failure and profile make the same semantic request safe;
- resume a provider only from an exact persisted continuation;
- reconcile a dispatched tool whose terminal observation was lost;
- restart a turn after a revision-changing mutation;
- request missing independent authority without dispatching the proposal;
- stop when a hard budget or structural limit is exhausted; and
- treat terminal failure, cancellation, and completion as closed.

Diagnostics carry operation, stable code, retry/recovery class, and bounded numeric facts. They do
not carry credentials, raw prompts, unrestricted model output, raw tool output, or provider-native
reasoning bytes.

## Verification and qualification

The verified domain proves the legal phase relation, deterministic reduction and replay,
no-implicit-success property, proposal non-authority, bounded accounting, capability/exposure
gating facts, stable tool-result ordering, and completion eligibility. Hashing, SQLite, async
providers, dispatchers, and OS/runtime handles stay in the audited ordinary half of the H-class
crate.

Focused development checks are:

```text
CARGO_BUILD_JOBS=2 cargo test --package peritus-agent --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy --package peritus-agent --all-targets --all-features --locked \
  -- -D warnings
CARGO_BUILD_JOBS=2 RUSTDOCFLAGS='-D warnings' cargo doc --package peritus-agent \
  --all-features --no-deps --locked
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-agent --all-features --locked \
  --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

The D0 fake-provider/fake-tool matrix covers complete inspect/edit/run/test loops, duplicate and
out-of-order streaming, interruption and legal retry, malformed tool calls, authority denial,
tool failure, pause/resume, cancellation at every active phase, crash replay, budget exhaustion,
bounded parallel execution, and non-accepting completion proposals. The complete merge authority
remains local Gate A plus the required hosted Gate A and Foundation matrices on Linux, macOS, and
Windows.

## Subsequent boundaries

D0 is one inner agent turn. D1 owns the gate engine, D2 owns reviewer/fixer finding lifecycle and
adjudication, D3 owns scheduling and collaboration, and E0 composes their outer workflow. A3, C7,
E1-E3, F0, G0-G3, and H0-H4 also remain separate production slices.
