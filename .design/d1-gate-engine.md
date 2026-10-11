# Feature: D1 Gate Engine

## Summary

Add a production `peritus-gates` orchestration crate that binds a validated immutable acceptance
contract to one exact revision and one clean C1 snapshot, plans gates in the contract's proven
topological order, executes only through the authorized `peritus-tools-quality` boundary, and
persists every state transition through C0 before it can influence acceptance. The engine is
fail-closed: only complete, fresh, well-formed candidate evidence can produce a passing gate.

## User-visible behavior

- A caller starts one gate run for a checked acceptance contract and exact `RevisionTuple`.
- The engine exposes the next deterministic set of runnable gates. A gate is never runnable before
  every declared dependency passed for the same exact run revision.
- Each attempt is recorded durably before dispatch. An acknowledged C4 quality result is decoded
  into a closed outcome vocabulary and recorded durably before scheduling changes.
- Candidate failures, infrastructure failures, cancellation, timeout, malformed output, and
  incomplete evidence remain distinguishable to callers and operators.
- Restart reconstructs the same state from canonical events. An attempt that was dispatched without
  a durable result is reconciled; it is never silently repeated or marked successful.
- The terminal summary is deterministic. All required gates must have fresh complete pass evidence;
  otherwise the run is failed, cancelled, blocked, exhausted, or indeterminate with typed reasons.

## Requirements

1. Bind every run to `AcceptanceContract::bind` and reject any revision or contract-digest mismatch.
2. Consume only `GateGraph::execution_order`; preserve its deterministic dependency ordering.
3. Bind each contract gate to one explicit `peritus-tools-quality::CheckDefinition` with the same
   `GateId`, deadline, execution environment, frozen action/input/parser/resource references, and
   supported success rule.
4. Require a physically distinct C1 `ReadOnlyWorkspace` whose snapshot lineage, generation, and
   revision exactly match the run revision and whose Git status is clean before dispatch.
5. Never call a process, shell, Cargo, or native sandbox directly from D1. Effects cross a bounded
   executor port whose production observation is a typed decoding of `quality.run`'s `ToolResult`.
6. Persist a canonical event and complete checkpoint atomically through C0 for every accepted
   command. Use C0 command identity/digest semantics for idempotent retries after lost acknowledgments.
7. Persist attempt intent before effect dispatch and persist its terminal observation before the
   gate may satisfy dependencies or acceptance.
8. Classify success, candidate failure, infrastructure failure, cancellation, timeout, malformed
   output, and incomplete evidence as distinct closed outcomes. Only success is passing.
9. Enforce the contract's nonzero per-gate attempt limit. Retry only retryable terminal outcomes,
   only after prior attempt reconciliation, and only with globally fresh execution and action
   identities durably retained in the canonical state. Total attempts may not exceed the production
   codec collection ceiling of 65,535.
10. Propagate cancellation to active executions, prohibit new dispatch after cancellation begins,
    and terminate only after every active effect has a durable terminal or recovery classification.
11. Block dependents deterministically when a prerequisite reaches a non-retryable or exhausted
    terminal. Aggregate in canonical gate order independent of observation arrival order.
12. Publish normalized gate evidence with exact revision, execution, attempt, result, artifact,
    C0 event, and clean-snapshot provenance. Artifact references must be complete and finalized.
13. Rebuild the run projection from genesis and verify the checkpoint's sequence, last event,
    revision, counters, and state digest against replay.
14. Keep public collections bounded, fields private, errors typed and source-preserving, and APIs
    callable from ordinary Rust without hidden Verus-only preconditions.
15. Supply executable Verus refinements for dependency ordering, exact freshness, attempt bounds,
    terminal truth, replay-equivalent state digests, and no implicit success.

## Acceptance criteria

- Valid multi-level DAGs schedule each gate exactly once per attempt after all dependencies pass.
- Duplicate, missing, self-referential, cyclic, mismatched, or unbound descriptors are rejected
  before any effect is requested.
- A stale revision, dirty/mismatched snapshot, missing result, malformed structured result,
  incomplete/truncated artifact, cancelled run, timeout, infrastructure failure, or lost process
  observation cannot produce a pass.
- Candidate failure blocks downstream gates and yields the same terminal summary after replay.
- Retryable failures can retry only below the configured attempt limit; candidate failures and
  explicitly non-retryable infrastructure failures cannot be retried. Reusing either an execution
  ID or action ID with an otherwise fresh retry is rejected without changing state.
- Cancellation is idempotent and recovery distinguishes never-dispatched, dispatched-active,
  durably-terminal, and indeterminate attempts.
- Replaying the exact committed event chain reproduces byte-identical canonical state and terminal
  aggregation; reordered, duplicated, gapped, stale, or digest-tampered frames are rejected.
- Quality result decoding validates descriptor identity, prepared-call identity, gate identity,
  closed status/failure consistency, completeness flags, result digest, progress truncation, and
  complete artifact provenance. Status, failure category, retryability, recovery route, and the
  structured outcome must match the closed quality matrix; contradictions are malformed and
  non-retryable.
- Focused unit/integration tests exercise inspect/edit/run/test quality flows through existing test
  authority fixtures, prerequisite failure, parser corruption, stale revision, cancellation,
  crash recovery, idempotent commit resolution, artifact publication, and snapshot enforcement.
- Focused formatting, strict Clippy, rustdoc, ordinary API checks, and no-cheating Verus verification
  pass before workspace Gate A.

## Current architecture

- B2 acceptance declarations are validated in `crates/foundation/peritus-spec`. `GateGraph`
  provides a proven deterministic topological order and `CompletionPolicy` provides the attempt cap.
- Exact-revision acceptance evaluation is pure in `crates/foundation/peritus-quality-policy`.
- C1 creates retained candidate snapshots and exposes immutable `ReadOnlyWorkspace` inspection in
  `crates/runtime/peritus-workspace`.
- C2/C3 execution authority and lifecycle live in `peritus-process` and `peritus-sandbox`; D1 must
  not recreate their permits.
- C4 `peritus-tools-quality` already discovers explicit checks, compiles restricted C2 plans, runs
  through the tool router, parses bounded complete output, publishes process artifacts, and emits a
  structured `ToolResult`. Its candidate projection is currently internal and needs a strict public
  terminal decoder plus clean-snapshot binding helper for D1.
- C0 `peritus-journal` supplies exact-frame atomic append, checkpoint CAS, command idempotency, and
  integrity export. Its aggregate vocabulary does not yet include D1 Gate.
- C0 `peritus-evidence` admits evidence only against checked journal provenance, exact revisions,
  and finalized artifact dependencies.

## Proposed design

### Domain and planning

`descriptor` projects the immutable contract graph plus explicit quality definitions into bounded
`GateDescriptor` values. Construction compares every frozen execution binding and produces a
canonical `GatePlan`. The plan retains gate IDs, dependency IDs, required evidence IDs, descriptor
digests, retry limits, and execution order without copying process authority.

### Pure state machine

`command`, `event`, `state`, and `reducer` form a closed transition system:

- start run;
- prepare, dispatch, observe, and reconcile an attempt;
- schedule a legal retry;
- request and finish cancellation;
- publish gate evidence; and
- finalize the run.

Commands carry predecessor sequence/event/state-digest fences. The reducer calculates the complete
successor and event. Illegal phases, identities, revisions, counters, retries, dependency states,
and outcome transitions are rejected. Preparation appends both execution and action identities to
bounded append-only ledgers; neither identity can be consumed again anywhere in the run. `replay`
applies the same transition rules to canonical events and checks each advertised successor digest.

### Effect shell

`engine` follows a prepare/commit/effect/observe/commit sequence. `GateExecutor` receives a bounded
`GateDispatch` containing identities and an immutable C1 snapshot reference; it returns only a
typed quality observation or explicit executor failure/recovery state. `EvidencePublisher` receives
an inert, completely provenance-bound publication request after the terminal event commits. Neither
port can authorize run state.

### Durability and projection

`durability` encodes version-one D1 event/checkpoint frames, appends them with the Gate aggregate
head expectation, installs the checkpoint atomically, and binds finalized artifact dependencies.
Replay observations retain their originating C0 `StoreId`; engines bind that identity at start or
resume and reject every later journal-taking operation against another store before reduction or
publication. The store binding is runtime durability authority and does not alter domain event,
checkpoint, or state-digest bytes.
An idempotency resolution is accepted only with the exact event and exact successor checkpoint; a
later checkpoint requires replay. The engine distinguishes a new append from a resolution and mints
an effect permit only for a newly committed dispatch. Loading checks every C0 record field before
replay and validates the installed checkpoint against the replayed result. `projection` derives
canonical run status and per-gate status solely from the event stream; it is rebuildable and has no
authority to dispatch.

Evidence publication creates a domain-separated canonical manifest over run, gate, complete attempt
and artifact provenance, revision, result event/global position/result digest, snapshot, ordered
requirements, and the exact complete quality artifact set. Publishers must finalize those manifest
bytes and construct the receipt from that exact request. The engine reloads the result event from
the bound authoritative journal and requires complete equality with the supplied record before
using its store-wide position. It then compares the returned request binding before
returning it, and the reducer reconstructs the same binding before permitting pass, including when
the gate declares zero evidence records.

Each requirement consumes one distinct admitted record projection. Duplicate evidence identity,
record digest, or producing position/event provenance is rejected independently of requirement
order. C0 evidence records do not currently embed a B2 `EvidenceRequirementId`; therefore this
slice enforces exact ordered one-to-one discharge without claiming an unavailable semantic
requirement-to-record proof.

### Quality integration

`peritus-tools-quality` gains two small stable surfaces:

- a deterministic acceptance binding for each `CheckDefinition`; and
- a strict decoder from its own `quality.run` `ToolResult` to a closed typed result.

The decoder treats absent/malformed structured fields, a mismatched descriptor/prepared/gate/result
identity, truncated progress, indeterminate output, incomplete artifacts, and inconsistent status as
non-success. A plan validator checks `WorkingDirectory` is read-only, is the exact immutable C1
snapshot root, and matches all revision/environment/resource identities before creating the existing
`QualityRunDispatcher`.

### Formal verification

`verified` defines mathematical projections of scheduling, freshness, bounded attempts, terminal
truth, replay folding, and pass predicates. Executable helpers used by ordinary Rust carry postconditions
connecting implementation booleans/counters to those specifications. Runtime tests cover the same
boundary with adversarial values.

## Data and compatibility

- New D1 frames are schema version 1 with reserved families 50 (command), 51 (event), and 52
  (checkpoint). Core transition semantics use closed typed fields; bounded byte fields are limited
  to content-addressed ancillary provenance that is independently digest-checked.
- Family 52 contains equal-length, unique execution-ID and action-ID ledgers bounded to 65,535;
  both ledgers participate in state hashing, checkpoint equality, and replay.
- `AggregateKind::Gate` receives a new stable append-only tag. Existing tags and rows do not change.
- Check-definition binding uses a versioned, domain-separated canonical digest. Any definition field
  affecting execution or interpretation changes the digest and invalidates reuse.
- Event and checkpoint decoders reject unknown critical tags. Future ancillary fields require a new
  schema version or explicitly compatible envelope.
- No existing C4 behavior changes; the new public decoder reads the already-emitted structured
  result and the snapshot validator narrows the accepted production construction path.

## Failure handling

- Input/contract/descriptor/snapshot failures occur before dispatch and are correctable typed errors.
- Candidate failure is authoritative evidence that the frozen check did not pass and is not retried
  automatically.
- Cancellation and timeout are terminal for the attempt and retryable only when run policy and a
  fresh authorized action permit it.
- Infrastructure and artifact failures preserve responsible subsystem, retryability, and recovery
  route. Indeterminate effects must be reconciled before retry.
- C0 append uncertainty is resolved by command ID plus request digest and the exact successor
  checkpoint. A later checkpoint forces aggregate replay and an exact resolved dispatch never
  recreates an effect permit. A conflicting command is an integrity failure; definitely absent can
  be safely re-appended with the same command.
- C0/frame/checkpoint/evidence mismatch quarantines the run from further execution.

## Security considerations

- D1 never owns raw process, shell, filesystem-mutation, sandbox, or policy permits.
- Debug and errors expose bounded identities/classifications, not captured stdout/stderr, tool
  arguments, workspace bytes, environment values, or secrets.
- Only a C1 immutable no-follow snapshot root can be selected for a gate.
- Artifact bytes are referenced by verified digest; a pass requires complete publication and exact
  prepared-call provenance.
- External observations cannot mutate authoritative state until validated and committed as events.

## Verification

Focused checks, serialized by the root agent:

1. `cargo test -p peritus-tools-quality -p peritus-gates`
2. `cargo clippy -p peritus-tools-quality -p peritus-gates --all-targets --all-features -- -D warnings`
3. `cargo doc -p peritus-tools-quality -p peritus-gates --no-deps` with `RUSTDOCFLAGS=-D warnings`
4. focused no-cheating Verus commands registered for the affected crates
5. A2 D1 conformance/integration selection once shared catalog registration is available
6. generated schema/fixture consistency and `xtask` architecture/trust/source-layout/ordinary-API
7. full workspace Gate A after D1 and C7 integration

## Rollout and rollback

The new crate is initially additive. Rollout requires workspace/architecture/B3/A2 registration by
the integration owner. Before any production write, the application must open a C0 store supporting
the Gate aggregate and D1 schema families. Rollback stops new D1 dispatch and retains existing
append-only events; readers remain able to replay schema v1. Removing registrations while v1 data
exists is prohibited.

## Open questions

- None. D1 B3 families 50/51/52 and `AggregateKind::Gate` tag 7 are reserved by the integration
  owner.
- The application composition point that owns construction of the authorized C4 executor is not yet
  present; D1 will expose a narrow port and production adapter inputs without expanding D0 scope.

## Out of scope

- Changes to workspace membership, architecture policy, shared B3/C0 files, generated protocol
  assets, A2 shared catalogs, CI, release notes, or Git/Crosslink state; the root integration owner
  handles them.
- Reimplementation of C1 candidate creation, C2 process supervision, C3 sandboxing, C4 routing, B1
  policy/lease/approval authority, or the pure acceptance evaluator.
- Any implicit shell/process fallback, unregistered parser execution, stale-result reuse, or
  acceptance override.
