# Feature: B0 Lifecycle Kernel

## Summary

B0 adds `peritus-kernel`, the pure verified authority core for Peritus lifecycle state. The crate
owns one causal aggregate containing session, run, attempt, turn, action, review, waiver, and
acceptance state machines. Its reducer accepts typed commands, checks B1 policy/budget witnesses
and B2 acceptance evidence against the aggregate's exact current `RevisionTuple`, and returns an
immutable next-state/event plan. It performs no persistence or effects.

## User-visible behavior

- A session can be opened, paused, resumed, and closed through explicit legal transitions.
- A run can start under the session's exact revision and budget, then be paused, resumed,
  cancelled, failed, exhausted, rejected, or accepted.
- Attempts, turns, actions, reviews, and waivers preserve their parent identities and follow
  explicit non-skippable phase transitions.
- An action becomes authorized only when a B1 capability-use transition names the exact action,
  digest, actor role, environment, and current revision.
- A run becomes accepted only when the kernel itself evaluates its current B2 contract and
  evidence and receives an acceptable result.
- Every accepted command emits one event whose sequence advances exactly once and whose causal
  predecessor is the aggregate's current head.

## Requirements

1. `KernelAggregate` contains exactly one session identity, project identity, contract binding,
   current revision, event cursor, and canonical collections of child lifecycle records.
2. `KernelCommand` is exhaustive for session, run, attempt, turn, action, review, waiver, and
   acceptance transitions. Every command has a stable `KernelCommandKind`.
3. `CommandEnvelope` binds one unique command ID, event ID, expected predecessor, and exact
   revision to one reducer invocation.
4. Rejected commands return a typed `KernelError` and do not produce a next state or event.
5. Accepted commands produce exactly one `KernelEvent` and a `KernelTransition` whose next
   aggregate head and sequence match that event.
6. Commands reject stale revisions, stale causal heads, duplicate command/event IDs, unknown or
   mismatched parents, duplicate child IDs, illegal phases, and sequence overflow.
7. Starting a run or attempt requires an open B1 budget snapshot for the exact current revision;
   a child budget must fit within the supplied parent availability.
8. Authorizing an action requires a B1 `CapabilityUseTransition` for the exact current action ID,
   action digest, actor, role, environment, and revision.
9. Acceptance is calculated inside the reducer by calling B2 `evaluate_acceptance` with the
   aggregate's exact current contract, revision, and supplied current evidence. A raw
   `AcceptanceDecision` is never an input command.
10. Failure, cancellation, exhaustion, rejection, pause, shutdown, or recovery-oriented
    transitions cannot produce `Accepted`.
11. The executable reducer refines an explicit Verus step relation and preserves aggregate
    validity, event-sequence monotonicity, parent causality, exact revision freshness, and no
    implicit success.
12. Public fields remain private; callers use constructors, accessors, and typed outcomes.

## Acceptance criteria

- `peritus-kernel` is registered as owner B0, layer `foundation`, verification class `V`, and
  depends only on B1/B2 `V` crates plus `peritus-types` and `vstd`.
- The full happy-path writer to action to review to acceptance trace succeeds.
- The transition matrix contains a positive and negative case for every command family and every
  terminal run phase.
- Reference-model generated traces agree with the executable reducer after every accepted step
  and on the first rejection.
- Adversarial tests cover stale revisions, wrong parents, duplicate identities, causal forks,
  policy witness mismatch, budget mismatch/exhaustion, stale acceptance evidence, and every
  non-acceptance terminal path.
- Verus verifies the initializer, reducer refinement, legal-transition preservation, exact event
  advancement, and no-implicit-success claims with `--no-cheating`.
- Focused Rust tests, strict Clippy/rustdoc, architecture/trust checks, and workspace Gate A pass.

## Current architecture

`peritus-types` already owns all required nominal IDs, one-based `EventSequence`, and
`RevisionTuple`. B1 supplies verified policy capability-use transitions and hierarchical budget
snapshots. B2 supplies immutable `AcceptanceContract`, exact `ContractBinding`, current evidence,
and the pure acceptance evaluator. `architecture.toml` reserves current-state, revision-freshness,
budget-ceiling, and current-evidence refinements for B0. No persistence or public protocol exists
yet; B3 and C0 consume the frozen B0 API later.

## Proposed design

### Aggregate and state records

`KernelAggregate` is the only authoritative B0 state. It stores the session state, optional current
run, canonical historical child records, current contract binding, current revision, and a causal
cursor. Child records are append-only identities with phase updates; terminal records are retained
for replay and diagnostics. The reducer rejects duplicate identities rather than silently
overwriting records.

The session phases are `Open`, `Paused`, and `Closed`. Run phases are `Pending`, `Running`,
`Paused`, `Reviewing`, `Fixing`, `Accepted`, `Rejected`, `Cancelled`, `Failed`, and `Exhausted`.
Attempts, turns, actions, reviews, waivers, and acceptance each use their own closed phase enum so
illegal cross-family states are unrepresentable.

### Command and event flow

`reduce(aggregate, envelope, command, inputs)` first validates the exact revision and causal head,
then dispatches to one command-family transition. A successful family transition mutates only a
local next-state value. The common reducer appends the command/event identities, advances the
event cursor once, emits the corresponding typed event, validates the next aggregate, and returns
the plan. No event is emitted on rejection.

`ReducerInputs` borrows current authority facts that cannot safely be serialized as raw commands:
the immutable acceptance contract, optional B1 capability-use transition, optional B1 budget and
parent-budget snapshots, and optional B2 acceptance evidence. Each command declares the inputs it
requires; unexpected inputs are ignored as non-authorizing context, while missing or mismatched
required inputs are rejected.

### B1/B2 integration

Run and attempt admission bind B1 budget IDs, phases, limits, parent availability, and exact
revision into kernel records. Action authorization derives its witness only from the checked B1
transition and stores the exact transition digest and scope facts. Acceptance commands never take
an `AcceptanceDecision`; the reducer binds the supplied contract to the current revision and calls
`evaluate_acceptance` itself. Only the acceptable branch can produce `RunPhase::Accepted` and
`AcceptancePhase::Accepted`.

### Verification model

The executable model exposes specification predicates for aggregate validity, legal phase edges,
causal head matching, exact sequence advancement, current-revision binding, and accepted-state
completeness. Proof functions establish initializer validity and reducer postconditions. Property
tests compare runtime traces to a deliberately smaller independent reference model for semantic
coverage outside the directly proved core.

The alternative was one independently sequenced aggregate per session/run/attempt/action. That
would require cross-aggregate atomicity and durable commit semantics before C0 exists, complicate
acceptance causality, and give B3 several provisional command surfaces. One aggregate is preferred
for B0; C0 may build independent projections without changing lifecycle authority.

## Data and compatibility

There is no stored production data or released protocol. B0 introduces the first kernel API. B3
will encode these commands/events after the API freezes. Event discriminants, error kinds, phase
enums, and field meanings are treated as compatibility-sensitive from B3 onward.

## Failure handling

Errors identify the first deterministic failure: revision, causal predecessor, duplicate
identity, missing parent, parent mismatch, illegal phase, missing/mismatched authority input,
budget constraint, acceptance unmet, aggregate invariant, or sequence overflow. Failure does not
return a speculative event or next state. Terminal failure/cancellation/exhaustion paths remain
observable and cannot be converted to success by replay.

## Security considerations

Commands are requests, not permissions. Only exact B1 witnesses can authorize actions or budgeted
work, and only an in-reducer B2 evaluation can accept a run. Revision and causal checks fail closed.
The crate has no unsafe code, I/O, ambient clock, randomness, secrets, process execution, or
persistence authority.

## Verification

Focused commands:

```text
cargo fmt --all -- --check
cargo test --package peritus-kernel --all-targets --all-features --locked
cargo clippy --package peritus-kernel --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-kernel --all-features --no-deps --locked
cargo verus verify --package peritus-kernel --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
just gate-a
```

## Rollout and rollback

B0 lands as one new crate plus workspace, architecture, proof-inventory, and CI registration. It
has no migration or runtime rollout. Rollback is removal of those additions before B3 depends on
the API; after B3 freezes the protocol, incompatible rollback requires a protocol change.

## Open questions

None block implementation. C0 remains responsible for durable currentness and commit receipts;
B0 outputs logical next-state/event plans only.

## Out of scope

- Durable journal append, replay storage, idempotency across compacted history, and commit receipts
  (C0).
- Gate execution and evidence provenance (D1/C0).
- Full review finding resolution workflow beyond B0's review/waiver lifecycle projection (D2).
- Workspace, process, tool, provider, scheduler, and orchestration effects (C/D/E slices).
- Public wire encoding, compatibility fixtures, and client schemas (B3).
