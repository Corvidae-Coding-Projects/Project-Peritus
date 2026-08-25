# D1 gate engine

`peritus-gates` is the durable orchestration boundary between an immutable B2 acceptance contract,
a clean C1 snapshot, authorized C4 quality execution, and C0 journal/evidence storage. It does not
run commands, open a shell, mutate a workspace, grant authority, or infer acceptance from prose.

## Trust model

A run is fixed to one `GatePlan`, `RevisionTuple`, and `CleanQualitySnapshot::binding_digest`.
`GatePlan::new` requires every contract gate to have one explicit, required
`peritus-tools-quality::CheckDefinition`. It checks the gate identity, timeout, environment,
action, inputs, parser, success predicate, and resource declaration component by component. A
discovered-only or optional check cannot implement a required acceptance gate.

The pure reducer is the only writer of authoritative run state. Effect adapters return inert typed
observations; those observations influence scheduling only after a corresponding event and complete
checkpoint commit successfully in C0.

## Lifecycle

The caller follows this order:

1. Revalidate a physically distinct, clean, detached, read-only C1 snapshot with
   `CleanQualitySnapshot::inspect`.
2. Construct `GatePlan`, then create a fenced `StartRun` command and call `GateEngine::start`.
3. For a runnable gate, commit `PrepareAttempt` with fresh execution and action identities.
4. Commit `MarkDispatched` before calling `GateEngine::execute`. This ordering ensures a restart
   can distinguish an effect that may exist from an attempt that was never dispatched.
5. Convert the returned `DispatchReceipt` with `observed_result_kind`, build the next fenced
   command, and commit it.
6. For a passing result, call `publish_evidence` with the exact `CommittedRecord` returned for the
   result event. Convert the checked receipt into `PublishEvidence` and commit it.
7. When every gate is passing or non-runnable, commit `FinalizeRun`.

Commands carry the expected aggregate sequence, prior event, prior state digest, exact revision,
reserved successor event identity, and idempotent C0 command identity. A stale command is rejected;
callers must reload and replay instead of editing its fences.

## Closed outcomes and retries

| Outcome | Can satisfy a gate | Default scheduling consequence |
| --- | --- | --- |
| `Passed` | Only after exact evidence publication | `EvidencePending`, then `Passed` |
| `CandidateFailure` | No | Non-retryable `Failed`; dependents become `Blocked` |
| `InfrastructureFailure` | No | Retry, recovery, or failure according to typed C4 metadata |
| `Cancelled` | No | Retry/failure while active; cancelled during run cancellation |
| `TimedOut` | No | Retry/failure according to typed C4 metadata |
| `MalformedOutput` | No | Fresh action only when explicitly classified retryable |
| `IncompleteEvidence` | No | Fresh action or recovery; never a partial pass |

`RetryPermission::AfterRecovery` always enters `RecoveryPending`. The `GateRecovery` adapter must
classify the old execution as still active, safe to retry, or terminal failure. A fresh attempt is
illegal until `SafeToRetry` is durable, must use new execution and action identities, and must remain
below the contract attempt cap. D1 retains both identity ledgers in canonical state, so a new
execution ID cannot disguise reuse of old action authority. Total run attempts are bounded to the
production codec ceiling of 65,535. Candidate failures are never automatically retried.

`GateEngine` holds a volatile, one-use effect permit only when that same live instance commits
`MarkDispatched`. `execute` consumes it before calling C4. A resumed engine has no such permit and
therefore cannot redispatch a possibly-owned effect; it must call `recover`. If cancellation is
committed before the live permit is consumed, the permit is discarded and the durable dispatched
attempt is reconciled like any other uncertain effect.

## Cancellation

`BeginCancellation` is idempotent. It prevents new effects, cancels work that has not acquired an
effect, and leaves dispatched/recovery-pending executions unresolved. The caller must record a
terminal result or recovery classification for every owned effect. `FinalizeRun` fails while any
gate remains prepared, dispatched, recovery-pending, retry-pending, evidence-pending, or otherwise
runnable. Missing observations never become successful results.

## Evidence

`publish_evidence` accepts the authoritative `SqliteJournal` and only the exact C0
`ResultObserved` record for the current passing execution. It reloads the run aggregate and
first requires the journal's `StoreId` to match the engine's durable store binding, then requires
complete record equality before trusting the supplied global position. A lookalike event from
another store cannot supply audit provenance. `EvidencePublication` binds the gate, execution,
revision, result event and global position, normalized result digest, clean-snapshot digest,
declared evidence IDs, and finalized quality artifact references. It exposes canonical manifest
bytes whose domain-separated digest also binds the run, complete attempt/action/prepared-call
provenance, artifact completeness, media type, label, size, and order. The publisher must finalize
that exact manifest and create its receipt with `EvidencePublication::receipt_from_records`. The
engine rejects a receipt from any other request; the reducer reconstructs the binding and checks the
exact result event, result digest, snapshot, artifact set, requirements, revision, gate, and
execution before permitting `Passed`. This applies even when the gate declares zero evidence
records.

Evidence discharge is one-to-one: distinct requirements cannot reuse an evidence identity, record
digest, or producing journal position/event provenance. Requirement order remains the exact
contract order. Current C0 evidence records do not carry a B2 `EvidenceRequirementId`, so D1 does
not claim a stronger semantic mapping than this exact ordered, unique discharge contract.

Artifact bytes remain in C0 artifact storage. Gate events contain only bounded content-addressed
metadata. Result artifact digests and evidence manifests are declared as C0 artifact dependencies,
so an event cannot commit before referenced bytes are finalized.

## Durability and restart

D1 owns schema-version-one canonical codecs:

| Family | Payload |
| --- | --- |
| 50 | Typed fenced gate command |
| 51 | Typed immutable gate event |
| 52 | Typed complete gate checkpoint |

The aggregate is `AggregateKind::Gate`; checkpoint namespace `0xD101` uses the stable run-derived
key returned by `gate_state_key`. Each accepted transition atomically appends one event and replaces
the complete checkpoint using head and state compare-and-swap.

`commit_gate_transition` first resolves the command identity and canonical command digest. Repeating
the same command after a lost acknowledgement returns its original checked committed batch only
when the event and complete successor checkpoint exactly match. If the aggregate has since advanced,
the stale caller receives `ReplayAggregate` and must reload the actual head. A resolved
`MarkDispatched` never recreates the live one-use effect permit, even when its checkpoint is still
exact; only the call that newly appends the dispatch may execute it. Reusing the command identity
with another digest is quarantined.

`GateReplay` retains the `StoreId` of the journal it loaded, and `GateEngine` binds that identity at
start or resume. Every later engine operation that accepts a journal checks the same store identity
before reduction, in-memory mutation, or publication. To move a run between physical stores, use a
separately verified migration workflow and load a new replay; do not substitute a journal handle on
an existing engine.

On restart:

1. Call `load_gate_replay` for the run ID.
2. Call `GateReplay::rebuild` with the immutable plan, or `GateEngine::resume`.
3. The loader checks every C0 record identity, sequence, predecessor, command, revision digest, and
   frame family. The pure reducer regenerates every event and state digest. The installed checkpoint
   must then match the rebuilt state field for field.

A codec, record, digest, sequence, checkpoint, identity ledger, or replay mismatch is an integrity
failure. Do not dispatch. Preserve the store and plan inputs for investigation.

The strict quality decoder also checks a closed matrix across terminal status, failure category,
retryability, recovery route, and structured candidate outcome. Contradictory or open combinations
become `MalformedOutput` with `Retryability::Never` and `RecoveryRoute::None`; they cannot authorize
a retry or pass.

## Error handling

`GateError` exposes a stable `GateErrorKind`, a bounded safe detail, an optional source, and the
required `GateRecoveryAction`:

- `CorrectInput`: fix a contract, descriptor, snapshot, identity, or command construction error.
- `ReplayAggregate`: reload C0 after a storage error or uncertain acknowledgement.
- `ReconcileAttempt`: determine whether the prior dispatched effect is still active or terminal.
- `FreshAction`: obtain new action authority only after the reducer permits retry.
- `RepublishEvidence`: repair finalized artifacts/evidence without rerunning a passing check.
- `Quarantine`: stop dispatch because integrity or replay truth cannot be established.

Errors and debug output intentionally omit command output, filesystem content, environment values,
and secrets.

## Operational invariants

The ordinary Rust predicates `attempts_are_bounded`, `dependency_order_is_legal`,
`dependencies_are_satisfied`, `evidence_is_fresh`, `no_implicit_success`, `terminal_truthful`, and
`replay_equivalent` make the safety checks available to tests and observers. Matching Verus proof
roots cover bounded increments, dependency ordering, exact freshness, terminal truth, replay
reflexivity, and the impossibility of success with missing result or evidence.

Read-only projections come from `GateProjection::from_state`. A projection has no execution,
evidence, retry, cancellation, or acceptance authority and may always be discarded and rebuilt.
