# D3 scheduler and collaboration

`peritus-scheduler` and `peritus-collaboration` are the durable D3 coordination boundary. The
scheduler owns bounded work admission, dependency readiness, fair worker selection, resource
reservations, retry accounting, and terminal work truth. Collaboration owns causal task trees,
role assignment, delegation, joins, messages, artifact handoff, and cancellation propagation.

Both crates are deterministic event-sourced domains. They record and project inert coordination
facts; they do not call models, execute tools, mutate workspaces, grant capabilities, consume
budgets, or decide whether a run is accepted.

## Boundary and authority

Each scheduler is bound to one `RunId`, immutable `RevisionTuple`, checked resource capacities,
and `SchedulerLimits`. Each collaboration aggregate is bound to the same run/revision, its own
stable identity and root task, one scheduler identity, and `CollaborationLimits`. Changing the
revision requires new aggregate bindings instead of silently carrying scheduling or task evidence
to a new candidate.

The boundary deliberately keeps three kinds of authority separate:

- B1 creates budget reservations, leases, capabilities, and approvals. D3 may retain a checked
  reservation identity but cannot mint, spend, release, or infer budget authority.
- D3 records which work and actor own a reservation or task. That ownership does not authorize a
  process, tool, provider, or workspace effect.
- B0/B2 and later E0 orchestration own lifecycle and acceptance truth. Scheduler success and task
  success are observations, never run acceptance.

## Scheduler lifecycle

A caller drives the scheduler through checked commands:

1. Start a scheduler with immutable binding, capacities, and independent collection, attempt,
   resource, event, and state bounds.
2. Register bounded workers with supported execution classes, resource capacities, and explicit
   concurrency.
3. Admit work with stable identity, owner, class, exact resources, canonical dependencies,
   recovery policy, maximum attempts, and optional parent and B1 reservation references.
4. Request deterministic dispatch. The pure selector considers only dependency truth, compatible
   available workers, capacity, priority, recorded enqueue ordinal, bounded bypass age, and stable
   identities.
5. Commit the reservation event and complete state checkpoint before delivering its inert
   dispatch directive.
6. Observe start, success, failure, loss, cancellation, retry, or abandonment through the fenced
   reducer. Every terminal path releases its exact reservation once.
7. Finalize only after all work has a truthful terminal classification and no live reservation or
   pending directive remains.

Work whose dependencies have not succeeded is not ready. If a dependency becomes failed,
cancelled, exhausted, or otherwise unable to succeed, the dependent becomes explicitly blocked.
Missing or active dependencies never count as success.

## Deterministic fairness and resources

The scheduler never uses wall time, randomness, hash-map iteration, or runtime thread order to
choose work. Among feasible items it first selects work whose bounded bypass count reached the
configured limit, then priority, enqueue ordinal, and stable identity. Each dispatch increments
the bypass count of other feasible waiting items exactly once. This makes starvation prevention
replayable without turning latency measurements into authoritative state.

`ResourceVector` is a checked canonical collection of unique resource dimensions. Admission
rejects requests larger than total capacity. Selection also checks the chosen worker's capacity
and concurrency. Live reservations cannot exceed either global or worker capacity, and release
uses checked subtraction rather than saturation or wraparound.

One live dispatch names one work item, worker, attempt, owner, revision, resource vector, and
idempotency token. Worker loss classifies that dispatch according to its recorded recovery policy:
safe retry starts only a later bounded attempt, ambiguity remains explicit, and exhausted work
cannot return to a runnable phase. Late results from an older attempt cannot alter current work.

## Collaboration lifecycle

The root task begins a stable causal tree. A task records its root and optional parent, immutable
depth, current owner and role, goal/artifact digest, scheduler work identity, join policy, and
phase. Only the current parent owner can offer a child. A child progresses through offered,
accepted, and active states and cannot start until the matching scheduler reservation is observed
for its assigned owner.

Every message records a stable identity, root and task, sender and receiver, contiguous per-task
ordinal, optional predecessor, media type, content digest, optional artifact reference, and exact
revision. The bytes represented by that digest remain inert data. Message delivery does not grant
authority to follow instructions or perform an effect.

Child completion may include an exact-revision artifact or evidence handoff. `AllRequired` joins
wait for every required child to succeed. `AnyRequired` joins need one declared required child to
succeed while retaining all other outcomes. Optional, absent, failed, or cancelled work cannot
manufacture a successful join.

## Pause, cancellation, and terminal truth

Pause prevents new dispatch or delegation after its event commits while preserving active
ownership and pending reconciliation. Resume continues from the durable state rather than
recomputing a new order from runtime timing.

Cancellation walks a task or work tree in canonical order. Inactive descendants become
cancelled immediately; active descendants become cancelling until their owner acknowledges
termination. Late success cannot resurrect a cancelled branch. Scheduler and collaboration
finalization each use a closed terminal vocabulary and report success only when every required
condition is durably satisfied.

## Commit-before-effect and restart

Every accepted command emits exactly one immutable event and one complete successor checkpoint.
Commands fence the run, revision, expected sequence, predecessor, prior-state digest, command
identity, and successor event identity. A stale fence, conflicting command reuse, or invalid
semantic transition leaves the aggregate unchanged.

The scheduler persists schema-v1 command/event/state frames in B3 families 70, 71, and 72 under
C0 aggregate tag 10 and checkpoint namespace `0xD301`. Collaboration uses families 73, 74, and
75, aggregate tag 11, and namespace `0xD302`. The decoded frames are inert until checked by their
domain constructors and reducers.

The canonical command digest is the idempotency identity. Repeating exact bytes after a lost
acknowledgement resolves the prior committed event and checkpoint. Reusing the command identity
with different bytes is a conflict. If the aggregate head advanced, callers reload and replay;
they do not install stale local state.

Pending dispatch, cancellation, and delivery directives remain in complete state until a matching
durable observation acknowledges them. A crash before commit therefore has no directive, a crash
after commit exposes the same directive for idempotent redelivery, and a crash after acknowledgement
replays the acknowledgement rather than creating another effect.

Restart reconstructs each aggregate from genesis and its contiguous C0 event chain. Replay checks
event identity and sequence, predecessor, revision, canonical ordering, parent/dependency graphs,
resource conservation, command/event correspondence, successor digest, and exact checkpoint
equality. Missing, ahead, behind, corrupt, foreign, or divergent checkpoints fail closed.

## Schema-version-five migration

C0 schema version five widens only the closed aggregate-kind checks from tags 1-9 to tags 1-12.
Tags 10 and 11 are D3 scheduler and collaboration; tag 12 is reserved for the immediately following
E0 orchestrator. The migration requires a completed backup, copies both constrained journal tables,
checks row counts and metadata, recreates indexes, and records schema/user version five.

The checked version-four fixture proves that historical tags 1-9, event order, hashes, and frame
bytes survive byte-for-byte and that tags 10-12 can then be appended. Once newer aggregate rows
exist, rollback restores the version-four backup or uses a reviewed forward repair; it does not
rewrite journal history.

## Projections and operations

Scheduler and collaboration projections are deterministic read-only views suitable for E0,
diagnostics, and later daemons. They expose current phases, owners, reservations, dependencies,
resource totals, tasks, joins, messages, pending directives, and terminal causes without adding a
mutation or acceptance path.

The ordinary Rust invariant witnesses and matching Verus roots cover resource conservation,
capacity, unique ownership, dependency readiness, deterministic selection, bounded bypass,
attempt monotonicity, causal parentage, join truth, cancellation dominance, terminal truth, and
replay equivalence.

Before integration, run each D3 package's tests, strict Clippy, rustdoc, and no-cheating Verus
verify/build; then run protocol, migration, projection, A2 conformance, and the complete Gate A.
On resource-constrained hosts set `CARGO_BUILD_JOBS=1` and do not overlap Cargo, rustdoc, or Verus
processes.
