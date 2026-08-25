# Feature: E0 Production AcTor Delivery Orchestrator

## Summary

E0 adds `crates/orchestration/peritus-orchestrator` as the durable deterministic outer delivery
loop for Project Peritus. It composes the already-authoritative D0 agent turn, D1 gate engine, D2
review engine, and D3 scheduler/collaboration domains into the complete writer -> gates ->
reviewer -> fixer lifecycle. It is the only application route that may request B0 acceptance, but
it cannot mint acceptance: the request is made only after the B2 evaluator returns an acceptable
decision for exact current evidence and B0 independently accepts the matching transition.

The orchestrator is a pure run-scoped reducer plus a commit-before-effect driver. Its state binds
one immutable acceptance contract, run/attempt, current `RevisionTuple`, exact candidate/tree/
artifact identity, role ownership, child aggregate heads, retry and oscillation bounds, and every
pending directive/observation. External work is represented by stable D3 work/task bindings.
Outbox directives and acknowledgements make every handoff resumable after a crash without
guessing whether a provider, tool, gate, review, or kernel action happened.

The architecture verdict is **ready** after D3. This document corrects the active goal's slice
label without reducing its behavior: the frozen roadmap calls scheduler/collaboration D3 and the
AcTor delivery loop E0. Both are delivered sequentially by the same goal and migration.

## User-visible behavior

1. A caller starts one orchestrator aggregate from a checked `AcceptanceContract`, exact run and
   attempt, current candidate binding, explicit writer/reviewer/fixer ownership, and bounded
   completion policy.
2. E0 durably creates the writer D3 collaboration task/work directive. The writer executes through
   D0 and returns an inert completion proposal bound to the exact revision and transcript/evidence
   digests. Successful observation atomically installs the actual writer output plus the
   same-revision D1/D2 quality binding while retaining the already-active writer D3 identities;
   no gate or review binding guesses the writer result.
3. A valid writer proposal advances to D1 gates. A clean current D1 terminal advances to D2
   review; gate infrastructure failure, deterministic failure, cancellation, or exhaustion follows
   an explicit configured branch and never counts as success.
4. D2 receives fresh independent reviewer assignments. A completed current review with conserved
   findings advances to B2 evaluation. Open findings create a fixer handoff that contains only
   canonical finding identities and current evidence, not hidden reviewer reasoning.
5. A fixer runs through D0 under the fixer role. Its result must bind a new candidate/tree/artifact
   revision and its D2 response records. Every material candidate change invalidates prior gate,
   review, and acceptance evidence; E0 starts a new gates -> review cycle.
6. Retry, gate-attempt, review-cycle, fixer-cycle, total-revision, and repeated-finding limits are
   independently bounded. D2 oscillation/escalation signals terminate as needs-human or exhausted,
   rather than silently looping.
7. Pause commits before stopping new work, preserves the exact resumable phase, and issues bounded
   child pause directives. Resume is legal only after replay and reconciliation. Cancellation
   propagates through D3 and remains cancelling until every active child reaches a known terminal.
8. Restart replays E0 from genesis, verifies its complete checkpoint, loads referenced child
   projections, reconciles pending outbox/ack state, and re-emits only the same idempotent pending
   directive. It never advances from a merely plausible child state.
9. E0 assembles exact D1/D2/evidence/approval/waiver observations and calls B2. If and only if the
   returned `AcceptanceDecision` is acceptable, it durably records that certificate and requests
   B0 `BeginAcceptance`/`EvaluateAcceptance` with exact replay input references.
10. E0 reaches `Accepted` only after observing the matching durable B0 `AcceptanceAccepted` event.
    Needs-changes, rejection, failure, exhaustion, cancellation, or ambiguous integration states
    have distinct truthful terminal outcomes.

## Requirements

### Immutable binding, roles, and candidate identity

- **E0-R001:** `OrchestratorBinding::from_contract` shall retain `RunId`, `AttemptId`, immutable
  contract identity/digest, exact initial `RevisionTuple`, initial D1/D3 child run identities, D1
  plan digest, D2 policy/binding digest, D3 scheduler/collaboration identities and binding digests,
  and `OrchestratorLimits`. Writer completion rebounds the bootstrap same-revision cycle to its
  actual candidate's checked D1 plan and D2 binding while preserving live D3 identities. A later
  material revision advance installs an entirely fresh per-candidate cycle.
- **E0-R002:** `CandidateBinding` shall retain exact `RevisionTuple`, workspace snapshot identity,
  candidate digest, tree digest, optional patch/artifact identity, artifact digest, producer actors,
  producer ancestry, and a canonical complete binding digest. Material change requires a new
  binding; no field may be updated independently.
- **E0-R003:** `RoleOwnership` shall identify the orchestrator service actor, writer, fixer, and
  required reviewer pool with exact B1 `ActorRole`/C6 `HarnessRole` agreement. Writer/fixer must
  have mutating roles; reviewers must have fresh read-only profiles and satisfy D2 independence.
  E0 cannot widen any role's capability view.
- **E0-R004:** Handoffs shall name the source/destination phase, source/destination actor and role,
  exact candidate binding, D3 task/work identity, exact D0 turn identity for writer/fixer work,
  input artifact/evidence digests, and one stable idempotency identity. Hidden reasoning and
  unbound free-form instructions are not handoff inputs.
- **E0-R005:** `OrchestratorLimits` independently bounds revisions, writer cycles, fixer cycles,
  gate cycles, review cycles, handoffs, child directives, retained observations, artifact refs,
  state/event bytes, and cancellation reconciliation. Contract completion limits may tighten but
  never exceed compiled ceilings.

### Closed lifecycle and role handoffs

- **E0-R010:** The closed command vocabulary shall cover genesis, child directive publication,
  child dispatch acknowledgement, writer completion/failure, gate completion/failure, review
  completion/escalation, fixer completion/failure, candidate revision advance, acceptance
  evaluation, B0 acceptance observation, pause/resume, cancel/reconciliation, rejection, failure,
  exhaustion, and finalization.
- **E0-R011:** Every accepted command emits exactly one event and successor state, binding
  `CommandId`, `EventId`, run, attempt, exact current revision, expected sequence/predecessor,
  prior-state digest, command kind, and all referenced child sequence/state digests. Rejection
  returns the exact input state and no event.
- **E0-R012:** The legal active order is writer -> gates -> review -> acceptance evaluation, with
  review -> fixer -> revision advance -> gates as the only fix loop. No direct writer/fixer ->
  acceptance, gate -> accepted, or review -> accepted transition exists.
- **E0-R013:** A D0 completion counts only when the child turn is terminal-completed, its actor/
  role/task/work/run/attempt/revision match the pending handoff, its request is legal for the
  current phase, and all evidence references are current. A proposal remains data and cannot
  execute its requested action. Writer success carries both its changed candidate and the exact
  same-revision quality cycle derived from checked D1, D2, and D3 child bindings.
- **E0-R014:** Gate completion requires a D1 projection for the exact run/revision/plan digest,
  terminal `Completed`, fresh evidence receipts for every required gate, and no pending recovery.
  Deterministic gate failure may consume a bounded fix policy; infrastructure/ambiguous failure is
  not converted into a code defect or success.
- **E0-R015:** Review completion requires an exact current D2 projection. `Completed` with complete
  quorum/conservation may advance to evaluation. Unconserved findings create a fixer task;
  `NeedsHuman`, failure, cancellation, exhaustion, disagreement, or oscillation follow their exact
  non-success branch.
- **E0-R016:** A fixer completion must contain a checked `CandidateBinding` successor plus D2 fixer
  response identities covering every handed-off current blocking finding. It cannot close a
  finding. D2 reviewer confirmation or external waiver remains required.
- **E0-R017:** Revision advance retains all prior cycle history, marks its D1/D2/acceptance facts
  historical, increments the bounded revision/fix counters once, creates fresh exact D1 and D3
  single-revision child aggregates, and applies D2's explicit revision-advance binding without
  discarding finding history. Reusing an old candidate/tree/artifact tuple, child run, scheduler,
  or collaboration identity as a claimed new cycle is rejected.
- **E0-R018:** Repeated D2 finding fingerprints, non-improving severity, incompatible reviewer
  outcomes, configured stagnation, maximum fixer/review/revision count, or explicit budget
  exhaustion produces `NeedsHuman` or `Exhausted` according to the recorded cause. No unbounded
  retry path exists.

### Gate, review, acceptance, and B0 truth

- **E0-R020:** E0 shall use D1 and D2 public projections/observations rather than reimplementing
  gate success, review quorum, finding conservation, reviewer independence, waiver authority, or
  oscillation semantics.
- **E0-R021:** `AcceptanceCertificate::from_evaluation` shall be the only public constructor for an
  E0 acceptance certificate. It accepts the checked contract, exact current `AcceptanceEvidence`,
  and the `AcceptanceDecision` returned by `evaluate_acceptance`; an unacceptable decision cannot
  create a certificate.
- **E0-R022:** The certificate binds contract, revision, candidate, D1/D2 state digests, canonical
  evidence digest, evaluation result digest, and completion limits. Any child/revision advancement
  invalidates it before another lifecycle transition.
- **E0-R023:** The certificate commits distinct B0 Begin/Evaluate command and event identities plus
  the exact prior kernel head. After committing it, E0 emits durable idempotent requests for B0
  `BeginAcceptance`, observes the matching durable begun event, and only then emits
  `EvaluateAcceptance`. The kernel reducer receives the original contract and exact
  `AcceptanceEvidence`; E0 records no accepted terminal from a request or local boolean.
- **E0-R024:** `OrchestratorTerminal::Accepted` requires a durable B0 `AcceptanceAccepted` event
  naming the same run/revision and causally linked to the certificate/request. B0
  `AcceptanceNeedsChanges` returns to the bounded fix/review path or a truthful terminal.
- **E0-R025:** `Rejected`, `Failed`, `Exhausted`, `NeedsHuman`, and `Cancelled` are distinct closed
  terminal kinds with stable cause codes. Every terminal first settles or reconciles all owned
  children without changing its retained cause. No cancellation, malformed observation, stale
  evidence, missing approval, unresolved finding, gate failure, review failure, or reconciliation
  ambiguity may produce `Accepted`.

### Pause, cancellation, crash recovery, and directives

- **E0-R030:** E0 phases shall distinguish active, paused-with-resumable-phase, cancelling, and
  terminal. Pause prevents new directive creation after its commit and preserves existing pending
  work. Resume requires every referenced child projection to match the paused checkpoint.
- **E0-R031:** Cancellation commits an E0 cancelling event before D3 cancellation directives are
  delivered. The aggregate remains cancelling until scheduler/collaboration/D0/D1/D2 children are
  terminal or explicitly classified unreachable/ambiguous. Late success cannot override cancel.
- **E0-R032:** Every external action is represented by a `PendingDirective` with stable identity,
  destination aggregate/port, exact payload digest, maximum deliveries, delivery/ack state, and
  causal source event. A directive is removed only by a matching durable observation.
- **E0-R033:** The runtime driver order is reduce -> canonical encode -> C0 commit event/checkpoint
  and outbox -> publish directive -> acknowledge -> observe child result. It shall never mutate
  in-memory authoritative state or acknowledge a provider/worker before the matching C0 commit.
- **E0-R034:** Recovery loads E0 plus referenced D0/D1/D2/D3/B0 heads/checkpoints and classifies
  each pending directive as deliverable, acknowledged-awaiting-result, completed-awaiting-E0
  observation, stale-conflicting, or terminal-ambiguous. Only the first three can progress
  automatically.
- **E0-R035:** Exact command replay returns the already committed event/checkpoint. A conflicting
  command digest, child state ahead/behind the recorded reference, absent checkpoint, invalid
  outbox acknowledgement, or irreconcilable terminal mismatch fails closed with typed recovery.

### Durability, protocol, compatibility, and projection

- **E0-R040:** Schema-v1 command/event/state frames use B3 families 76, 77, and 78. Decoded frames
  remain inert until checked by E0 constructors/reducer; unknown tags and trailing bytes reject.
- **E0-R041:** C0 aggregate tag 12 is `Orchestrator`; namespace `0xE001` owns the complete run
  checkpoint. Migration v5 from D3 admits tags 10-12 without changing tags 1-9 or historical
  bytes. E0 adds no second schema widening.
- **E0-R042:** One accepted transition atomically appends its family-77 event, installs the full
  family-78 successor checkpoint, records exact artifact dependencies, and installs any new
  outbox directive. C0 head/state CAS and command resolution follow the D1/D2 pattern.
- **E0-R043:** Replay from genesis rejects sequence/predecessor gaps, duplicate IDs, stale current
  bindings, illegal handoffs, noncanonical actor/finding/evidence sets, counter regressions,
  unbound child observations, successor digest mismatch, and any accepted state without a valid
  recorded B0 observation.
- **E0-R044:** `OrchestratorProjection` exposes current phase, candidate binding, owners, cycle/
  limit counters, pending directives, child heads, last gate/review summaries, open handoff,
  cancellation progress, and terminal/cause. It has no mutation or acceptance authority.
- **E0-R045:** B3 schema/TypeScript output, complete binary fixtures and SHA256 manifests,
  architecture registry, C0 projections/migration fixtures, A2 orchestrator conformance,
  reproducibility checks, formal inventory, README, CHANGELOG, and operational docs are updated.

### Verus and maintainability

- **E0-R050:** Legal phase order, role separation, exact candidate freshness, evidence invalidation,
  bounded counters, no implicit acceptance, cancellation dominance, unique pending directive,
  terminal truth, and replay equivalence shall have executable Verus specs/proofs wherever the
  pinned toolchain supports the concrete state.
- **E0-R051:** The crate contains no `assume`, `admit`, axiom, trusted body, `unsafe`, hidden public
  precondition, placeholder, ignored test, state-machine macro, or convenience authority bypass.
- **E0-R052:** Public fields remain private; domain constructors are total and typed. `lib.rs` is
  below 80 lines. Production modules target 400 lines and never exceed 700. Domain, integration
  records, reducer, wire, durability, driver, recovery, and projections remain separate modules.
- **E0-R053:** E0 depends on public contracts from B0/B1/B2/C0/C1/D0-D3. It shall not depend on
  concrete provider adapters, process/shell tools, OS sandboxes, or workspace/Git implementations.

## Acceptance criteria

| Criterion | Required evidence |
|---|---|
| Exact immutable binding | one-field drift matrix for contract/revision/candidate/tree/artifact/child digests |
| Full happy path | writer -> gates -> independent review -> B2 certificate -> durable B0 accepted |
| Fix cycle | blocking finding -> fixer -> new revision -> gates -> reviewer confirmation -> accepted |
| Role handoffs | writer/reviewer/fixer ownership and D3 task/work binding positive/negative matrix |
| Evidence invalidation | every material revision field invalidates prior D1/D2/B2 certificate evidence |
| Bounded looping | writer/fixer/gate/review/revision/repeated-finding limits each terminalize independently |
| Terminal truth | scenario for every accepted/rejected/failed/exhausted/needs-human/cancelled cause |
| Crash recovery | failpoint after every commit/publish/ack/result boundary with exact replay/resume |
| Pause/cancel | every active phase, pending directive, active child, late completion, and restart case |
| Protocol/durability | families 76-78, tag 12, namespace, checkpoint equality, fixtures and digest conflicts |
| Conformance | nonempty A2 happy/fix/failure/replay/cancel/panic/teardown catalogs |
| Formal quality | strict no-cheating Verus, ordinary API audit, Clippy, rustdoc, targeted integration, Gate A |

## Current architecture

D0 owns one model/tool turn and emits `CompletionProposal`; it never accepts a run. D1 owns gate
planning/execution/evidence and exposes `GateProjection`. D2 owns review cycles/findings/quorum/
oscillation and exposes `ReviewProjection` plus B2 observations. D3 owns resource dispatch and
causal task ownership. B2 evaluates exact `AcceptanceEvidence`; B0 alone owns lifecycle
`AcceptanceAccepted`. C0 already supplies atomic multi-aggregate appends, checkpoints, artifact
dependencies, idempotency, and outbox delivery.

No current crate orders those boundaries into a production delivery lifecycle or reconciles them
after a crash. The B0 run phases anticipate running/reviewing/fixing/acceptance but deliberately do
not perform external orchestration.

## Proposed design

### Run aggregate and state flow

One `OrchestratorState` is keyed by `RunId` and stores the immutable genesis binding, current
candidate, ownership, sequence/head, current resumable phase, bounded counters, cycle history,
child references, pending directive, acceptance certificate, cancellation reconciliation, and
terminal. Only one directive may be pending because each handoff is durably acknowledged before
the next phase; D3 supplies concurrency inside a phase where policy permits it.

```text
Starting
  -> WriterPending -> WriterActive -> GatesPending -> GatesActive
  -> ReviewPending -> ReviewActive
       -> FixerPending -> FixerActive -> RevisionAdvancing -> GatesPending
       -> EvaluatingAcceptance -> KernelAcceptancePending -> Accepted

Any active state -> Paused(resumable)
Any nonterminal state -> Cancelling -> Cancelled
Bounded failures -> Rejected | Failed | Exhausted | NeedsHuman
```

### Durable composition shell

The core reducer consumes inert `ChildObservation` records whose constructors verify the relevant
public projection/terminal and complete binding. The driver owns ports for journal/outbox,
scheduler, collaboration, D0, D1, D2, B2 evaluation, and B0 commit. Ports expose plans and checked
observations, not concrete provider/process/workspace implementations. Each driver call performs
at most one E0 transition and one idempotent delivery/observation step, making interruption and
testing precise.

Acceptance uses two proofs of truth: E0 can create an `AcceptanceCertificate` only from the B2
evaluator's checked acceptable decision, then E0 becomes accepted only from the resulting durable
B0 event. This avoids making a decoded E0 frame or caller boolean acceptance authority.

### Closed command and event surface

Representative commands are:

```text
Start
PublishDirective
AcknowledgeDirective
ObserveWriter
ObserveGates
ObserveReview
ObserveFixer
AdvanceCandidate
RecordAcceptanceCertificate
ObserveKernelAcceptance
Pause
Resume
Cancel
ReconcileCancellation
Reject
Fail
Exhaust
Finalize
```

Events mirror accepted semantic facts rather than requests. `Finalize` computes terminal truth;
it never accepts a caller-selected terminal kind.

### Module layout and frozen ownership

```text
crates/orchestration/peritus-orchestrator/
  Cargo.toml                         # root integrator
  README.md                          # root integrator
  src/
    lib.rs                           # root integrator
    identity.rs                      # core/formal worker
    limits.rs                        # core/formal worker
    binding.rs                       # core/formal worker
    candidate.rs                     # core/formal worker
    ownership.rs                     # core/formal worker
    handoff.rs                       # core/formal worker
    phase.rs                         # core/formal worker
    command.rs                       # core/formal worker
    event.rs                         # core/formal worker
    state.rs                         # core/formal worker
    state/mutation.rs                # core/formal worker
    reducer.rs                       # core/formal worker
    reducer/apply.rs                 # core/formal worker
    terminal.rs                      # core/formal worker
    verified.rs                      # core/formal worker
    child.rs                         # integration/durability worker
    acceptance.rs                    # integration/durability worker
    directive.rs                     # integration/durability worker
    canonical.rs                     # integration/durability worker
    wire/{mod,command,event,state}.rs # integration/durability worker
    durability.rs                    # integration/durability worker
    durability/binding.rs            # integration/durability worker
    replay.rs                        # integration/durability worker
    projection.rs                    # integration/durability worker
    runtime/{mod,driver,recovery,ports}.rs # integration/durability worker
  tests/
    domain_*.rs                      # core/formal worker
    integration_*.rs                 # integration/durability worker
    durability_*.rs                  # integration/durability worker
    crash_matrix.rs                  # integration/durability worker
```

After D3 is integrated, the same two implementation agents may be reassigned to these disjoint
core/formal and integration/durability paths. The root retains all shared workspace/C0/B3/A2/docs/
formal files, crate root/manifests, integration review, commands, Git, and hosted verification.

### Alternatives considered

Letting B0 call D0/D1/D2 directly would move effectful application sequencing into the verified
lifecycle kernel and violate its pure authority boundary. E0 instead requests and observes B0
transitions through existing durable adapters.

Using a saga with only database flags and no event reducer would be shorter but could not prove
legal ordering, exact replay, cancellation dominance, or no implicit acceptance. The explicit
event-sourced aggregate is preferred.

Combining D0, D1, D2, D3, and E0 into one crate would create a god boundary, duplicate existing
tests, and make independent scheduler/evaluation reuse impossible. E0 composes stable projections
and directives while each domain retains its own truth.

## Data and compatibility

Families 76-78, aggregate tag 12, namespace `0xE001`, command/event/phase/terminal tags, canonical
field order, handoff/directive identity bytes, and certificate digest composition become immutable
on merge. Unknown tags are rejected. Historical cycles and candidate bindings are append-only.
Checkpoints/projections may migrate but must reproduce genesis replay exactly.

## Failure handling

- Invalid/stale child input returns a typed error and emits no E0 event.
- A crash before E0 commit creates no directive; after commit it exposes the same idempotent
  directive; after child completion it reconciles the exact child projection.
- Deterministic quality/review needs-changes is distinct from infrastructure or ambiguous failure.
- Irreconcilable child/E0 head disagreement quarantines recovery; it is never guessed into success.
- Limit exhaustion preserves complete cycle/finding/candidate history for diagnosis.
- Cancellation and pause preserve authoritative state while bounded effect cleanup proceeds.

## Security considerations

The orchestrator's B1 role has inspection/execution coordination but no raw effect, acceptance,
waiver, or policy authority. It cannot execute handoff text, mint capabilities, approve evidence,
or fabricate reviewer independence. Candidate artifacts are exact digest dependencies checked by
C0. The implementation focuses on realistic stale/malformed/conflicting observations, resource
loss, crashes, and cancellation races rather than speculative unrelated adversaries.

## Verification

Targeted E0 domain, wire, durability, runtime, crash, conformance, C0/B3, Clippy, rustdoc, and
strict Verus checks run serially with `CARGO_BUILD_JOBS=1`, followed by `cargo xtask all` and one
full `just gate-a`. No heavyweight local commands overlap. Hosted Gate A/Foundation matrices run
on Linux, macOS, and Windows; one isolated rerun is allowed only for an evident runner timing
failure before code changes.

## Rollout and rollback

E0 lands with D3 through signed commits and a protected PR. Migration v5 requires a completed v4
backup and preserves all old rows/bytes. Removing E0 after tag-12 data exists is not a compatible
downgrade; rollback restores the v4 backup or uses a later forward repair. Final completion
requires merged `main == origin/main`, a fresh-main serialized local Gate A, and green hosted
fresh-main Gate A/Foundation.

## Open questions

None. Slice naming, sequencing, acceptance authority, role ownership, tags, migration, driver
ordering, bounded loops, recovery behavior, and verification requirements are fixed here.

## Out of scope

- CLI, TUI, daemon IPC/supervision, packaging, and remote fleet consensus.
- Provider-specific prompts/parsing and concrete workspace/process/tool execution.
- Harness materialization/evolution, sealed evaluation campaigns, and promotion policy.
- New acceptance, waiver, budget, capability, or reviewer-independence authority.
