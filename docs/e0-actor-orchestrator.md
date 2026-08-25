# E0 AcTor delivery orchestrator

`peritus-orchestrator` owns the durable outer delivery loop that composes D0 agent turns, D1
quality gates, D2 independent review, D3 scheduling and causal collaboration, B2 quality
evaluation, and B0 lifecycle truth. It orders those authorities; it does not replace them.

## Authority boundary

E0 may create work and task directives, validate observations, advance an exact candidate
revision, request B2 evaluation, and submit an acceptable certificate to B0. It cannot call a
provider directly, execute a tool, mutate a workspace, issue a waiver, decide reviewer
independence, reinterpret a gate result, or mark a run accepted.

The only successful terminal path is:

```text
writer -> gates -> independent review -> B2 acceptable decision
       -> durable B0 acceptance request -> matching durable B0 AcceptanceAccepted event
```

A fixer response returns to a new revision and fresh gates. It never closes D2 findings or skips
reviewer confirmation. Every rejection, failure, exhaustion, needs-human result, or cancellation
is retained as its own terminal fact.

## Immutable run binding

One aggregate binds the B2 acceptance contract, B0 run and attempt, initial revision, initial D1
gate run, initial D3 scheduler and collaboration runs, role ownership, and independent limits. A
per-candidate quality-cycle binding retains the exact D1 child run and plan, D2 binding, and D3
child runs, identities, and binding digests. D1 and D3 are immutable single-revision aggregates,
so every successor candidate receives fresh child aggregates; D2 keeps its run and uses its
explicit revision-advance transition so finding history remains conserved. Writer completion
atomically installs its actual changed candidate and rebounds the same-revision quality cycle to
the now-known D1 plan and D2 binding while retaining the D3 identities already executing that
handoff. Later fixer revisions require fresh D1 and D3 aggregates. The current candidate
additionally binds its workspace snapshot, candidate and tree digests, optional patch/artifact
identity, artifact digest, producer actors, and producer ancestry.

Material candidate change creates a complete successor binding. It cannot update one digest in
place. Advancing the revision retains cycle history and invalidates prior D1, D2, B2, and B0
evidence for current acceptance.

Writer, fixer, reviewer, and service ownership is explicit. Handoffs bind the source and target
phase, actor and role, exact candidate and revision, D3 work and task, the exact D0 turn for writer
and fixer work, evidence inputs, and stable idempotency identity. Free-form output is inert data
and cannot widen a role or authorize an effect.

## Lifecycle

The normal active phases are:

```text
Starting
  -> WriterPending -> WriterActive
  -> GatesPending -> GatesActive
  -> ReviewPending -> ReviewActive
       -> FixerPending -> FixerActive -> RevisionAdvancing -> D3 quiescence -> GatesPending
       -> EvaluatingAcceptance -> D3 quiescence -> KernelAcceptancePending -> Accepted
```

The reducer accepts one causally fenced command and emits exactly one immutable event and successor
state. It checks command identity, expected sequence and predecessor, prior-state digest, run,
attempt, current revision, and referenced child heads. Rejected input leaves the state unchanged
and emits no event.

D0 completions count only for the exact pending handoff and matching actor, role, task, work, run,
attempt, revision, and evidence. D1 completion must be the current plan and revision with complete
fresh required receipts and no pending recovery. D2 completion must be the current binding with
complete independent quorum and finding conservation. E0 consumes their public projections rather
than implementing a second opinion about their results.

## Bounded correction

Writer cycles, fixer cycles, gate cycles, review cycles, revisions, handoffs, child directives,
retained observations, artifact references, state size, event size, and cancellation
reconciliation are independently bounded. Contract limits can tighten compiled ceilings but cannot
widen them.

Repeated finding fingerprints, stagnant or worsening severity, incompatible review outcomes,
explicit budget exhaustion, or any cycle limit produces an exact `NeedsHuman` or `Exhausted`
cause. Infrastructure or ambiguous failure is not relabelled as a code defect and never counts as
success.

## Directives and commit ordering

Every external request is a `PendingDirective` with a stable identity, exact destination and
payload digest, bounded delivery count, acknowledgement state, and causal source event. At most one
directive is pending for an E0 aggregate. D3 owns any permitted concurrency inside the delegated
phase.

The driver order is fixed:

1. reduce one inert command or checked child observation;
2. canonically encode the event, complete checkpoint, and optional outbox directive;
3. atomically commit them through C0;
4. publish only the committed directive;
5. durably acknowledge delivery; and
6. later consume the matching checked child result.

A process crash cannot turn an in-memory plan into authority. Retrying the exact committed command
or directive resolves idempotently; reusing an identity with different canonical bytes is a
conflict.

## Pause and cancellation

Pause records the resumable phase before preventing new directives. Existing committed child work
remains explicit. Resume is legal only after replay and after every referenced child projection
matches the paused checkpoint.

Cancellation first commits `Cancelling`, then publishes bounded child cancellation directives. The
aggregate remains cancelling until each active D0-D3 child is terminal or an exact current-revision
classification retains nonzero evidence that it is unreachable or ambiguous. Such classifications
are durable non-success observations and can never express acceptance. Rejection, failure,
exhaustion, and needs-human causes use the same
settling path while retaining their own truthful terminal classification. A late child success
cannot override cancellation or another already-committed non-success cause.

## Recovery

Restart replays E0 from genesis, validates the complete checkpoint, and loads referenced B0 and
D0-D3 child heads. Each pending directive is classified as deliverable, acknowledged and awaiting
result, completed and awaiting observation, stale-conflicting, or terminal-ambiguous. Only the
first three classes progress automatically.

Missing checkpoints, sequence or predecessor gaps, duplicate identities, child head drift,
checkpoint inequality, invalid acknowledgements, and irreconcilable terminal states fail closed.
Operators preserve the journal and child aggregates for diagnosis instead of guessing a successor
state.

## Protocol and persistence

E0 uses schema-version-one B3 families 76, 77, and 78 for command, event, and complete state. The
C0 aggregate kind is `Orchestrator` with permanent tag 12; complete checkpoints use namespace
`0xE001`. Schema migration version five admits D3 tags 10 and 11 plus E0 tag 12 while preserving
historical tags 1-9 and their bytes.

Unknown versions or tags, malformed lengths, noncanonical collections, and trailing bytes are
rejected. Decoded frames remain inert until checked constructors and the reducer validate their
authority-bearing bindings.

## Acceptance truth

E0 can construct an acceptance certificate only from the exact checked acceptance contract,
current `AcceptanceEvidence`, and an acceptable `AcceptanceDecision` returned by B2
`evaluate_acceptance`. The certificate binds current candidate, revision, D1 and D2 state digests,
canonical evidence, evaluation result, and completion limits.

The certificate commits an exact two-envelope B0 plan: `BeginAcceptance` binds its command, event,
and prior kernel head, while `EvaluateAcceptance` binds its distinct command and event to the
planned begin event. E0 observes the durable begun event before issuing evaluation and records
`Accepted` solely after the matching `AcceptanceAccepted` event with the same run, revision,
certificate, and causal chain. A local boolean, decoded frame, emitted request, or successful child
cannot mint acceptance.

## Operational verification

Use the focused E0 domain, integration, codec, durability, recovery, crash, and A2 conformance
tests before the repository gate. Strict Clippy, rustdoc, ordinary-safe API auditing, no-cheating
Verus verification, formal trust accounting, generated artifacts, and the complete Gate A remain
merge requirements. On resource-constrained hosts set `CARGO_BUILD_JOBS=1` and never overlap
Cargo, rustdoc, Verus, xtask, or `just` processes.
