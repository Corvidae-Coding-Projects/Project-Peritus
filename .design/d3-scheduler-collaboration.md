# Feature: D3 Production Scheduler and Collaboration

## Summary

D3 adds `crates/orchestration/peritus-scheduler` and
`crates/orchestration/peritus-collaboration` as the deterministic resource-coordination and
causal-delegation substrate used by later delivery, evaluation, daemon, and evolution slices.
Both crates are run-scoped, event-sourced orchestration domains. They accept checked inert
commands, emit one immutable event and one complete successor checkpoint per accepted command,
and recover exclusively through C0 journal replay. Neither crate runs a model, executes a tool,
mutates a workspace, grants authority, or decides acceptance.

The scheduler owns bounded fair queues, canonical dependency readiness, worker ownership,
resource reservations, backpressure, retry accounting, pause/drain/cancellation, and truthful
terminal state. The collaboration crate owns stable task identities, root/parent causality,
delegation and join semantics, bounded fan-out/depth/messages, role ownership, artifact handoff,
and cancellation propagation. A collaboration task binds one scheduler work identity, but the
two aggregates remain separate: E0 or a later daemon coordinates their already-durable commands
without allowing either domain to impersonate the other.

The architecture verdict is **ready**. D3 remains the scheduler/collaboration slice from the
frozen production roadmap. Writer -> gate -> review -> fixer control belongs to E0 and is designed
separately in `.design/e0-actor-orchestrator.md`.

## User-visible behavior

1. A caller creates one scheduler aggregate for a run with immutable revision, capacities, and
   explicit bounds, then registers bounded worker descriptors.
2. Work is enqueued with a stable identity, owner, execution class, exact resource request,
   optional B1 budget-reservation reference, canonical dependencies, and optional parent work.
3. The pure scheduler chooses the next feasible item deterministically. Dependencies must have
   succeeded, a compatible available worker must exist, and every requested resource must fit.
4. A committed reservation names the exact work, worker, attempt, resource vector, and dispatch
   token. Only after that event is durable may an effect shell deliver the inert dispatch.
5. Successful, failed, lost, cancelled, or exhausted work releases its reservation exactly once.
   Recoverable worker loss may create a bounded new attempt; ambiguous or exhausted work remains
   explicit and never becomes success.
6. Bounded-bypass aging prevents feasible queued work from being perpetually displaced by later
   priority work. Infeasible work is rejected at admission; dependency-blocked work reaches an
   explicit dependency terminal when a prerequisite cannot succeed.
7. Collaboration creates a root task and delegates child tasks with stable root/parent identity,
   assigned actor/role, exact revision, scheduler work binding, bounded depth, and join policy.
8. Messages retain sender, receiver, task, causal predecessor, ordinal, content/artifact digest,
   and media type. Delivery is an inert durable fact, not authority to execute the content.
9. Parent completion waits for all required children. Optional children remain explicit and may
   not manufacture parent success. A child handoff binds exact output artifact/revision evidence.
10. Pause prevents new dispatch/delegation while preserving active ownership. Cancellation marks
    every queued descendant immediately and every active descendant as cancelling until its owner
    acknowledges termination. Restart reconstructs the exact same state and pending directives.

## Requirements

### Scheduler identity, resources, and admission

- **D3-R001:** `SchedulerBinding` shall retain `RunId`, immutable `RevisionTuple`, scheduler
  identity, canonical `SchedulerLimits`, and canonical resource capacities. Any revision change
  requires a new scheduler aggregate.
- **D3-R002:** `WorkId`, `WorkerId`, `DispatchId`, `CollaborationId`, `CollaborationTaskId`, and
  `CollaborationMessageId` shall be private-field nonzero 16-byte identities with total checked
  constructors and stable canonical byte representation owned by their domain crates.
- **D3-R003:** A `ResourceVector` shall be a nonempty canonical collection of unique
  `ResourceKind`/`ResourceQuantity` entries. Addition and subtraction are checked; no reservation,
  release, delegation, or replay path may wrap, saturate, underflow, or create capacity.
- **D3-R004:** Admission shall reject an empty or oversized request, duplicate identity,
  unsupported execution class, unknown dependency/parent, cycle, self-dependency, request greater
  than capacity, parent/revision mismatch, noncanonical collection, zero attempt bound, or any
  compiled/configured bound violation before durable admission.
- **D3-R005:** `SchedulerLimits` shall independently bound queued work, total retained work,
  workers, dependencies per work, resource dimensions, active reservations, attempts per work,
  bypass count, event/state bytes, and dispatch batch size. Production ceilings shall bound every
  allocation reachable from decoded input.
- **D3-R006:** An optional `BudgetReservationId` is an observed B1 binding only. D3 shall not
  create, alter, consume, release, or infer a budget reservation and shall never treat resource
  availability as budget authority.

### Deterministic scheduling and worker ownership

- **D3-R010:** The closed scheduler command vocabulary shall cover genesis, worker registration/
  availability/drain/loss, work admission, deterministic dispatch, start acknowledgement,
  success/failure, recoverable retry, pause/resume, work/tree cancellation, scheduler drain,
  exhaustion, and finalization.
- **D3-R011:** Every accepted command emits exactly one event and successor state and binds
  `CommandId`, `EventId`, run, revision, expected sequence, predecessor, prior-state digest, and
  complete command payload. Stale fences and conflicting command reuse fail without transition.
- **D3-R012:** Readiness requires every declared dependency to be terminal-successful. A failed,
  cancelled, or exhausted dependency deterministically terminalizes the dependent as blocked;
  missing or merely active dependencies never count as ready.
- **D3-R013:** Selection shall be a total deterministic ordering over feasible work: forced aged
  items first, then priority, enqueue ordinal, and identity. A feasible waiting item bypassed by a
  dispatch increments exactly once up to the configured bound; at the bound it precedes non-aged
  work. Tests shall compare the implementation with an independent reference selector.
- **D3-R014:** A worker may own at most one active dispatch unless its descriptor explicitly
  declares a greater concurrency count. The sum of all live reservations shall not exceed any
  global or worker capacity dimension. Each live dispatch maps one-to-one to one work attempt.
- **D3-R015:** Dispatch is commit-before-effect. A `WorkReserved` event contains the complete
  dispatch directive and idempotency token. Replay exposes an undelivered or unacknowledged
  reservation for idempotent redelivery; it never creates a second reservation.
- **D3-R016:** Completion, acknowledged cancellation, worker loss, and explicit abandonment
  release an exact reservation once. Duplicate terminal observations are exact idempotent replays
  or rejections; a late result for an older attempt cannot change current work.
- **D3-R017:** Worker loss classifies each active dispatch as safely retryable, ambiguously
  terminal, or failed according to its recorded recovery policy. Retry creates the next bounded
  attempt only after release; exhaustion yields `WorkTerminal::Exhausted`.
- **D3-R018:** Scheduler finalization reports `Completed` only when all admitted work succeeded,
  no reservation remains, and no directive is pending. Any failed/dependency-failed/ambiguous/
  exhausted work yields the corresponding non-success terminal. Cancellation never completes.

### Collaboration causality, delegation, and join

- **D3-R020:** `CollaborationBinding` shall retain one run/revision, root task, scheduler aggregate
  identity, and `CollaborationLimits`. Every task shall resolve to exactly one acyclic parent chain
  ending at the root and carry its immutable depth and one canonical scheduler `WorkId`.
- **D3-R021:** A task assignment shall name one owner `ActorId`, one `HarnessRole`, a parent owner,
  a goal/artifact digest, and a closed `JoinPolicy`. Only the current parent owner may delegate a
  child; role/profile validation remains a C6/B1 observation and grants no capability.
- **D3-R022:** Fan-out, depth, retained tasks, messages, recipients, payload bytes, and artifact
  references are independently bounded. A rejected over-limit delegation/message changes no
  state and emits no event.
- **D3-R023:** A message shall bind stable identity, root/task, sender, receiver, per-task ordinal,
  optional predecessor message, media type, content digest, optional artifact reference, and exact
  revision. Predecessors must already exist in the same task; ordinals are contiguous.
- **D3-R024:** Delegation has an explicit offered -> accepted -> active lifecycle. Rejection,
  cancellation, failure, and abandonment remain terminal facts. A task cannot start before the
  bound scheduler work is reserved for the assigned owner.
- **D3-R025:** Task completion shall retain a terminal kind and optional exact-revision artifact/
  evidence handoff. `AllRequired` joins complete only after every required child succeeds;
  `AnyRequired` completes after a declared required child succeeds while all other child outcomes
  remain retained. No missing, optional, failed, or cancelled child can silently satisfy a join.
- **D3-R026:** Task cancellation propagates deterministically through all descendants in canonical
  order. Inactive descendants become cancelled; active descendants become cancelling until owner
  acknowledgement. Completion after cancellation cannot resurrect success.
- **D3-R027:** Collaboration finalization succeeds only when the root and every required joined
  descendant succeeded and no task/message delivery or cancellation is pending. Failure,
  abandonment, cancellation, or unsatisfied joins remain truthful non-success terminals.

### Durability, protocol, and compatibility

- **D3-R030:** Scheduler schema-v1 command/event/state frames use B3 families 70, 71, and 72;
  collaboration command/event/state frames use 73, 74, and 75. Decoded frames are inert and must
  pass domain constructors and reducers before they can affect authoritative state.
- **D3-R031:** C0 adds immutable aggregate tags 10 (`Scheduler`) and 11 (`Collaboration`) while
  preserving tags 1-9. Dedicated checkpoint namespaces are `0xD301` and `0xD302`, with
  domain-separated run-state keys.
- **D3-R032:** Each accepted domain transition atomically appends one event and installs its
  complete successor checkpoint under C0 head/state CAS. Exact command replay resolves the prior
  bytes; conflicting bytes, advanced checkpoints, or mismatched event/state fail closed.
- **D3-R033:** Replay rejects gaps, duplicate IDs, predecessor mismatch, invalid root/parent
  chains, resource imbalance, stale revisions, unknown tags, noncanonical collections, state
  digest mismatch, illegal semantic transitions, and trailing bytes.
- **D3-R034:** Migration version five widens aggregate-kind checks from 1-9 to 1-12 once for D3 and
  the immediately following E0 aggregate. It uses the established backup-required table-copy and
  row-count/meta checks, preserves historical bytes/order, and has an exact v4 compatibility
  fixture. Reserving tag 12 does not make E0 frames valid until its crate is present.
- **D3-R035:** B3 registry/schema/TypeScript output, binary fixture manifests, architecture
  ownership, reproducibility checks, formal inventory, and A2 scheduler/collaboration conformance
  catalogs shall be generated or updated from canonical sources.

### Verus and maintainability

- **D3-R040:** Resource conservation, capacity bounds, unique ownership, dependency readiness,
  deterministic selection, bounded bypass, attempt monotonicity, causal parentage, join truth,
  cancellation dominance, terminal truth, and replay equivalence shall have executable Verus
  specifications/proofs wherever the pinned toolchain supports the data structure.
- **D3-R041:** There shall be no `assume`, `admit`, axiom, trusted body, `unsafe`, hidden public
  caller precondition, placeholder, ignored test, or state-machine macro. Ordinary Rust executes
  the same checked bodies after ghost erasure and returns typed errors.
- **D3-R042:** Public fields remain private. `lib.rs` files are composition/export surfaces below
  80 lines. Production files target 400 lines and never exceed 700; domain, wire, durability,
  projection, and tests remain focused modules without generic `manager`/`utils` buckets.
- **D3-R043:** Neither production crate depends on a provider adapter, process/shell implementation,
  workspace mutator, approval issuer, or acceptance authority.

## Acceptance criteria

| Criterion | Required evidence |
|---|---|
| Admission and bounded resources | constructor matrix plus over-capacity, duplicate, cycle, and arithmetic boundary tests |
| Deterministic fair selection | independent reference-selector traces covering priority, aging, feasibility, and stable ties |
| Reservation conservation | Verus proof plus multi-worker allocation/release/retry tests |
| Crash-safe dispatch | failpoint matrix around commit, delivery, acknowledgement, result, and restart |
| Dependency truth | success DAG and each failed/cancelled/exhausted prerequisite outcome |
| Worker loss | retryable, ambiguous, exhausted, drained, and stale-result scenarios |
| Causal collaboration | root/parent/depth/fan-out/message-predecessor invariant proofs and rejection tests |
| Join and handoff truth | all-required/any-required/optional/failure/cancel/artifact-binding matrix |
| Cancellation trees | queued and active descendant propagation with replay at each phase |
| Compatibility | families 70-75, aggregate tags 10-11, v4->v5 migration preservation, fixtures and digests |
| Conformance | nonempty A2 catalogs with passing, failing, panic, teardown, replay, and cancellation subjects |
| Formal quality | strict per-package Verus no-cheating, ordinary API audit, Clippy, rustdoc, and Gate A |

## Current architecture

B0 already owns lifecycle causality and acceptance transitions. B1 owns budgets, leases,
capabilities, and approvals. B3 owns stable frame registration. C0 provides multi-aggregate
transactional event append, complete state checkpoints, artifact dependencies, and outbox rows.
D0 emits inert completion proposals; D1 and D2 produce exact-revision gate/review evidence. C6
owns role profiles and reviewer independence. D3 consumes those stable boundaries without
changing their authority.

No current crate provides fair queueing, worker assignment, resource reservations, causal task
delegation, joins, or cancellation trees. `peritus-budget` accounts governed consumption but is
not a worker scheduler. `peritus-agent` coordinates a bounded tool batch inside one turn but does
not schedule independent run work.

## Proposed design

### Scheduler aggregate and selection

One `SchedulerState` is keyed by `RunId`. It retains binding, limits, sequence/head, phase,
canonical workers, canonical work records, live reservations, resource totals, enqueue/dispatch
ordinals, and pending durable directives. `decide` clones validated state, applies one closed
command, recomputes derived readiness and resource totals, advances the cursor once, and emits an
event containing the successor digest. `replay` rebuilds the corresponding command from each event
and requires exact event equality.

Work phases are `WaitingDependencies`, `Queued`, `Reserved`, `Running`, `Cancelling`, and a closed
terminal. Worker phases are `Available`, `Busy`, `Draining`, `Lost`, and `Removed`. Reservations
exist only in `Reserved`, `Running`, or `Cancelling`. Selection is pure and uses recorded ordinals,
never wall time, randomness, map iteration order, or runtime thread scheduling.

### Collaboration aggregate

One `CollaborationState` per run retains the root task, all bounded task records, messages,
delivery acknowledgements, joins, and pending scheduler-cancellation directives. Canonical vectors
are used at the verified boundary; lookup indexes may be reconstructed outside the authoritative
state. Every child stores its root, parent, depth, owner, role, scheduler work, join membership,
and immutable revision. A task/message is never executable authority.

### Module layout and frozen ownership

```text
crates/orchestration/peritus-scheduler/       # scheduler worker only during D3 implementation
  Cargo.toml
  README.md
  src/
    lib.rs
    identity.rs
    limits.rs
    resource.rs
    worker.rs
    work.rs
    selection.rs
    command.rs
    event.rs
    state.rs
    state/mutation.rs
    reducer.rs
    verified.rs
    canonical.rs
    wire/{mod,command,event,state}.rs
    durability.rs
    durability/binding.rs
    projection.rs
    runtime.rs
  tests/
    domain_*.rs
    selection_*.rs
    durability_*.rs
    replay_*.rs

crates/orchestration/peritus-collaboration/   # collaboration worker only during D3 implementation
  Cargo.toml
  README.md
  src/
    lib.rs
    identity.rs
    limits.rs
    task.rs
    message.rs
    join.rs
    command.rs
    event.rs
    state.rs
    state/mutation.rs
    reducer.rs
    verified.rs
    canonical.rs
    wire/{mod,command,event,state}.rs
    durability.rs
    durability/binding.rs
    projection.rs
  tests/
    domain_*.rs
    causality_*.rs
    durability_*.rs
    replay_*.rs
```

The root integrator exclusively owns workspace manifests/lockfile, `architecture.toml`, B3
registry/generator output, C0 aggregate/migration/projection edits, A2 catalogs, formal manifests,
repository docs, Git operations, and all heavyweight commands. Workers do not edit shared files.

### Durability and recovery

The two durability adapters follow the established D1/D2 exact event-plus-checkpoint pattern.
Pending dispatch, cancellation, and message-delivery directives are retained in successor state
and may also be placed in the C0 outbox by the composition shell. An acknowledgement is a later
domain command. Therefore a crash before commit has no directive, a crash after commit exposes
the same idempotency token for redelivery, and a crash after acknowledgement replays the ack.

### Alternatives considered

A thread-pool API with in-memory channels would be smaller but would make scheduling order,
reservation ownership, cancellation, and restart depend on runtime timing. Rejected in favor of a
pure event-sourced plan plus a thin effect shell.

Combining scheduling and collaboration into one aggregate would make every message contend with
resource dispatch and would prevent independent reuse by E3. Separate aggregates with explicit
stable bindings preserve focused ownership and allow C0 to coordinate durable directives.

Wall-clock priority aging was rejected because replay would require reproducing timing. Bounded
dispatch-bypass counters provide deterministic fairness for feasible work; operational latency is
measured by C7/H3 rather than made authoritative here.

## Data and compatibility

Families 70-75, aggregate tags 10-11, namespaces `0xD301`/`0xD302`, command/event variant tags,
identity bytes, resource-kind ordering, and canonical field order become immutable on merge.
Unknown tags and trailing bytes are rejected. Historical events are append-only; projections and
checkpoints remain rebuildable caches.

## Failure handling

- Constructor/reducer errors are typed, bounded, and leave input state unchanged.
- Commit uncertainty resolves by command identity and canonical request digest.
- A missing/ahead/behind/different checkpoint quarantines the aggregate pending replay repair.
- Worker loss never guesses whether an ambiguous external effect succeeded.
- Capacity exhaustion causes backpressure or explicit exhaustion, not unbounded queues.
- Dependency, join, cancellation, and terminal outcomes never collapse into success.

## Security considerations

D3 coordinates already-authorized inert work; it grants no authority. Actor/role/resource/budget
references are exact observations. Message bytes and artifact references are bounded inert data.
The realistic failure surface is malformed/stale/conflicting input, worker loss, resource
pressure, cancellation races, and corrupt replay; unrelated speculative adversaries are not used
to inflate the implementation.

## Verification

The root runner serializes targeted scheduler tests/Verus, collaboration tests/Verus, affected C0/
B3/A2 checks, strict Clippy/rustdoc, `cargo xtask all`, and one full `just gate-a`, always with
`CARGO_BUILD_JOBS=1`. Worker agents perform no workspace-wide or overlapping heavy commands.

## Rollout and rollback

D3 and the following E0 land through signed commits on one feature branch because migration v5
reserves all three aggregate tags in one byte-preserving schema transition. Hosted Gate A and
Foundation matrices must pass on Linux, macOS, and Windows. A rollback restores the required v4
backup; once tag 10/11/12 records exist, downgrade requires backup restore or a forward migration.

## Open questions

None. Stable tags, namespaces, deterministic fairness, resource ownership, aggregate separation,
formal posture, and sequencing into E0 are fixed by this design.

## Out of scope

- Executing provider, process, tool, Git, or workspace effects.
- Issuing capabilities, budgets, approvals, waivers, or acceptance.
- Daemon worker processes, IPC transport, CLI/TUI surfaces, and distributed fleet consensus.
- E3 evaluation policy and F0 evolution policy.
