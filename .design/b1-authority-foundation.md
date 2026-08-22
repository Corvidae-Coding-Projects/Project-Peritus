# B1 authority foundation

- Status: frozen after independent architecture review
- Date: 2026-08-22
- Owner: B1
- Depends on: A1 formal foundation, A2 test/conformance foundation
- Unlocks: production B0, B3, C1-C4, C6, D0, D3, G3

Independent design evidence: three read-only `gpt-5.6-sol`/`xhigh` passes covering
policy/approval, budgets, and leases; two correction rounds; final verdicts ready on 2026-08-22.

## Summary

B1 establishes the authority boundary for Project Peritus. It adds four production crates:

- `crates/foundation/peritus-policy`, class V;
- `crates/foundation/peritus-budget`, class V;
- `crates/state/peritus-leases`, class H with a verified core;
- `crates/state/peritus-approval`, class H with a verified core.

The slice is complete only when policy evaluation, capability consumption, budget accounting,
lease fencing, and approval resolution have executable Verus models, ordinary-Rust safe wrappers,
typed failure behavior, adversarial tests, and reviewed proof evidence. The slice discharges the
pure-state forms of `INV-006 ExclusiveWriter`, `INV-007 RoleSeparation`, `INV-008 CapabilityScope`,
`INV-009 ApprovalReplaySafety`, `INV-012 BudgetMonotonicity`, and `INV-022
PolicyMonotonicity`. It records explicit downstream refinement obligations wherever durable journal
ordering, canonical action encoding, operation classification, or target resolution is required to
lift those claims to the integrated system.

This is not a temporary or reduced authority model. B1 defines the production domain contracts
that later persistence, workspace, process, tool, scheduler, agent, and UI slices must implement.

## User behavior

The eventual user-visible behavior derived from B1 is:

1. Every consequential operation is evaluated against an immutable effective policy and exact
   actor, role, environment, resource-operation pairs, revision tuple, and validity interval.
2. The result is exactly one of authorized, approval required, or denied. Approval can satisfy an
   approval requirement; it cannot override an explicit or compiled denial.
3. An issued capability is scoped, expiring, use-limited when configured, and bound to its
   issuance. B1 produces an exact logical-use transition for one matching action; B0 provides a
   verified current-kernel witness and C0 provides commit evidence. E0 assembles those inputs, but
   the target C1/C2/C4 authorization gateway independently validates the bundle and alone constructs
   its private effect permit after the transition is durably committed.
4. Reviewer/evaluator authority cannot mutate workspaces. Writer/fixer authority cannot accept,
   waive, amend protected policy, or promote harnesses.
5. Approval requests render bounded structured summaries, bind the responder and exact request,
   expire predictably, and cannot be replayed for another action, policy, or revision.
6. “Approve similar” authorizes one exact policy amendment candidate. It never produces an action
   permit. Applying it creates a new immutable policy ID, after which the action is evaluated again.
7. Tokens, cost, active-effect time, attempts, and retries are accounted monotonically. A retry
   cannot escape exhaustion, provider corrections cannot refund authoritative consumption, and
   ambiguous completion consumes the reserved ceiling.
8. At most one active writer lease exists for a workspace generation. Expiry or holder loss fences
   the old generation before recovery; another writer cannot acquire until reconciliation reports
   that the resource is safe.
9. Clock restart, regression, arithmetic overflow, stale state, malformed adapter observations,
   and indeterminate commits fail closed without constructing authority.

## Requirements

### Functional requirements

- B1-F-001: Policy decisions are total, deterministic, and whole-request. Partial grants are not
  returned.
- B1-F-002: Permission atoms are exact `(ResourceId, CapabilityName)` pairs. Resource and
  operation sets are not independently crossed.
- B1-F-003: Operation classification is read from authenticated policy configuration; a caller
  cannot self-label a mutation as an inspection.
- B1-F-004: Applicable deny decisions dominate every other decision. A lower layer is a
  restriction, not an independent grant source.
- B1-F-005: Effective policy is the protected authority ceiling intersected with every complete
  lower restriction layer. An ordinary lower layer cannot broaden authority.
- B1-F-006: An authenticated amendment may alter only its declared tier and remains bounded by
  every higher-tier ceiling and immutable denial.
- B1-F-007: Capability issuance and use preserve exact actor, role, environment, permission,
  revision, policy, validity, and remaining-use constraints.
- B1-F-008: Budget accounting covers model tokens, provider-cost microunits, accountable
  active-effect milliseconds, attempts, and retries as a fixed complete vector.
- B1-F-009: Budget consumption never decreases. Reservations may release only unused capacity;
  no API refunds consumed authority.
- B1-F-010: Parent/child budget delegation conserves capacity and propagates child consumption to
  every ancestor in the same successful transition.
- B1-F-011: Lease scope is one workspace lineage, resolved resource, and environment. Active
  ownership is exact actor and session identity.
- B1-F-012: Release, expiry, holder loss, clock discontinuity, and revocation fence the old lease
  generation before any later acquisition.
- B1-F-013: Lease authority is the intersection of a current exact capability and the current
  lease. Acquiring a lease alone never grants mutation authority.
- B1-F-014: Approval decisions bind the approver, request ID and digest, action digest, policy,
  revision tuple, choice, and validity interval.
- B1-F-015: An identical repeated approval response is idempotent; a conflicting response is
  rejected. One-time approval can be consumed at most once.
- B1-F-016: B1 exposes checked planning, transition, and observation contracts. It does not claim
  an effect or persistence succeeded. Adapter observations remain unprivileged evidence; a later
  durable-store composition must establish commit before it constructs an effect permit.

### Verification requirements

- B1-V-001: Every deterministic authority decision supported by pinned Verus is executable Verus
  code with a matching specification and proof.
- B1-V-002: Public ordinary-Rust entry points have no unchecked caller-visible `requires` clauses.
  They validate preconditions and return typed failures.
- B1-V-003: `state_machine!` may model transition reachability after the ordinary-API checker is
  narrowed to the exact pinned macro import. `tokenized_state_machine!` is prohibited because the
  pinned implementation introduces a trusted generated implementation.
- B1-V-004: No B1 class-V source contains `assume`, `admit`, axioms, `external_body`, external
  specifications, unsafe code, or proof exclusions.
- B1-V-005: Deterministic H-core exclusions are symbol-specific, independently reviewed, and
  compensated by refinement and differential tests. Broad crate or module exclusions are forbidden.
- B1-V-006: Rejected transitions preserve the prior state and construct no permit, capability,
  token, receipt that authorizes work, or terminal approval.
- B1-V-007: Formal evidence and proof-impact identities cover tests and manifests as compilation
  inputs, not only `src/` files.

### Rust and maintenance requirements

- B1-R-001: Public authority-bearing types have private fields and checked constructors and cannot
  be directly deserialized into privileged states.
- B1-R-002: Capability-use, lease-use, and approved-action logical transitions are non-`Copy` and
  non-`Clone`. The downstream effect permits composed from them inherit that requirement.
- B1-R-003: Every public failure has a stable subsystem code and an explicit retry/recovery class.
- B1-R-004: Recoverable failures never panic, wrap, saturate, or partially mutate state.
- B1-R-005: `lib.rs` files contain documentation, module declarations, and intentional re-exports
  only. There are no generic `common`, `helpers`, `manager`, `misc`, or `utils` modules.
- B1-R-006: B1 crates do not expose implementation dependencies or serialization types through
  their public APIs.
- B1-R-007: A2 crates are dev dependencies only. Unit and model tests are deterministic and require
  no network, sleep, live clock, or nondeterministic identifier source.

## Acceptance criteria

1. The four crates are registered at their canonical paths with exact B1 ownership, permitted
   dependency edges, and package-level verification commands.
2. `peritus-types` contains the reviewed shared primitives required by B1: `BudgetId`,
   `BudgetReservationId`, `RevisionTuple`, and `ResourceKind::AttemptCount`. A2’s exhaustive nominal
   ID fixtures remain exhaustive.
3. All B1 public fields are private; privileged values cannot be forged by struct literals,
   `Default`, direct decoding, or public unchecked constructors.
4. Every policy decision dimension has a negative test, including an adversarial permission-pair
   test that would expose Cartesian scope expansion.
5. Exhaustive bounded policy composition confirms deny dominance and proves adding an ordinary
   restriction layer never increases the allowed query set.
6. The complete role/operation matrix proves reviewer/evaluator non-mutation and writer/fixer
   non-acceptance/non-waiver/non-promotion.
7. Capability tests cover issuance, exact action binding, policy/revision staleness, half-open time
   boundaries, one and many uses, exhaustion, and failed-use nonconsumption.
8. Budget tests cover exact limit, one-over, mixed dimensions, partial cumulative observations,
   duplicate and decreasing observations, ambiguity, overrun faulting, retry accounting, nested
   child budgets, closure, and every checked arithmetic boundary.
9. Lease tests cover every legal and illegal state edge, two concurrent acquisition plans, renewal
   races, expiry equality, epoch mismatch, holder loss, generation/version exhaustion, corrupt CAS
   observations, indeterminate commits, and old-claim rejection after every fence.
10. Approval tests cover every legal and illegal state edge, field-by-field digest tampering,
    credential and signer mismatch, expiry before and after decision, response replay, one-time
    consumption, conflicting decisions, amendment bounds, and safe rendering.
11. Generated command traces are compared with independent reference models after every accepted
    and rejected transition. Failure seeds are persisted in test diagnostics.
12. `verification/obligations.toml` contains reviewed statements, owners, dependency edges, exact
    proof symbols, and evidence for all B1-owned invariants and supporting obligations.
13. Any conditional downstream aspect of an invariant is represented by a stable refinement ID in
    this design rather than hidden in a broad or overstated discharge claim. Its owning slice creates
    a manifest entry only after its package, source symbol, registered owner, and live issue exist;
    invalid placeholders are forbidden.
14. No new trusted construct is present. `verification/trust.toml` remains empty unless an
    independently approved design amendment identifies an unavoidable trusted boundary.
15. The independently reviewed B1 proof-change record is merged into protected `main` before the
    implementation change asks Gate A to accept the corresponding formal-source fingerprints.
16. `just gate-a`, clean workspace Cargo-Verus verification/build, ordinary tests, doctests, strict
    Clippy, docs, architecture checks, trust checks, proof-manifest checks, and compatibility checks
    pass locally and in the protected pull-request matrix.
17. After merge, the exact protected-main commit passes the Gate A and Foundation workflows on
    Linux, macOS, and Windows where applicable.
18. Capability, budget-begin/reservation, lease, and approval logical transition outputs alone
    cannot satisfy any B1 effect API. B1 tests their private construction, exact binding, failed-
    transition nonproduction, and reusable forged/malformed/stale commit-claim fixtures. The named
    C0/E0/C1/C2/C4 refinements own compiled integrated permit/dispatch tests when those crates exist.
19. Approval validity fails at exact equality for the request, credential, signed decision,
    escalation challenge, and their intersected result. B1 proves approve-similar emits only one
    candidate-bound logical approval and no action grant. The named E0 refinement owns activation,
    durable-commit, and subsequent-reauthorization integration evidence. A correctly authenticated
    response still cannot satisfy an explicit or compiled denial.
20. Approval cryptography passes pinned SHA-256 known-answer and RFC 8032 Ed25519 suites plus an
    alternate-implementation differential suite. Rendering rejects or safely represents control
    bytes and invalid UTF-8 sources, respects exact truncation bounds, passes seeded secret canaries,
    and never exceeds its configured output bound. Delayed decisions signed before revocation fail
    after same-key reissue because signed key ID, credential generation, and registry revision are exact.

## Current architecture

A1 provides class-V nominal IDs, `CapabilityName`, `Sha256Digest`, one-based revisions,
generations, event sequences, `ResourceKind`, and exact checked `ResourceQuantity` arithmetic.
Those types deliberately make no time, I/O, or authority decisions. A2 provides deterministic
clocks, identifiers, event builders, scripted calls, fault injection, repository fixtures, and a
conformance runner, but owns no B1 domain semantics.

The workspace currently has no policy, budget, lease, or approval crate and no B1 proof obligation
entries. The formal source inventory is exact-hash governed. After A1 genesis, a new reviewed PCR
must already exist unchanged on protected `main` before a formal source can move to its recorded
identity. B1 therefore uses the same two-pull-request pattern proven by A2.

The production architecture assigns policy and budget to the foundation layer and lease/approval
state to the state layer. One registry correction is made explicit here: a package has one class,
so `peritus-leases` and `peritus-approval` are class H, not the informal table label “V/H.” Their
pure authority reducers and proofs remain Verus; their ports and unavoidable external algorithms
are ordinary Rust.

## Proposed design

### Dependency and authority direction

```text
peritus-types (V)
  ├── peritus-policy (V)
  ├── peritus-budget (V)
  ├── peritus-leases (H) ────── depends on exact policy permits
  └── peritus-approval (H) ──── depends on escalation/amendment policy values

peritus-test-support (C) ─────── dev dependency only
peritus-conformance (C) ──────── black-box B1 scenario registration only
```

`peritus-policy` does not depend on budgets. `peritus-budget` does not depend on policy. Their
integration theorem belongs to the future kernel: a root budget must be no broader than the
effective policy ceiling. `peritus-approval` may depend on `peritus-policy`; the reverse dependency
is forbidden. `peritus-leases` accepts a concrete exact policy-use transition, not an
authority-returning trait. That value contributes logical scope to the lease reducer but remains
insufficient for effect dispatch until the downstream commit composition.

The master crate table already declares production `peritus-kernel` dependencies on policy and
budget. Its separate statement that B0/B1/B2 may proceed in parallel is interpreted narrowly:
B0/B2 can develop and review independent models after shared A1 primitives freeze, but the
production B0 crate cannot freeze or register a replacement authority interface before B1. The
canonical architecture document is amended during integration to make that dependency explicit.

B1 owns the stable security roles and immutable role-separation predicate. C6 may add role-harness
context, prompts, presentation names, and selection metadata but cannot redefine B1 authority. B1
also owns the canonical `OperationDescriptor`/classification value. C4 consumes that value and
proves each concrete tool refines it; it does not create a second classification authority.

Each later effect subsystem owns its checked public gateway request type and private permit:
`WorkspaceAuthorizationRequest` in C1, `ExecutionAuthorizationRequest` in C2, and
`ToolAuthorizationRequest` in C4. E0 constructs those lower-layer request values; no gateway names
an E0-owned type. Each request contains borrowed/exact views of B1 logical transitions, the B0
current-state witness, and C0's opaque commit receipts. Consequently C1 and C2 explicitly depend on
C0 as well as B1/B3/A2, and C4 depends on C0 plus C1/C2 and B1/B3. The canonical slice dependency
graph is amended during B1 integration to record this ordering. This is intentional: an effect
gateway cannot truthfully authorize a committed transition before the durable receipt contract
exists.

### Shared A1 additions

`peritus-types` receives only time-independent, cross-slice primitives:

- `BudgetId` identifies one immutable hierarchical budget account;
- `BudgetReservationId` identifies one idempotent reservation/charge lineage;
- `ResourceKind::AttemptCount` distinguishes the first attempt from `RetryCount`;
- `RevisionTuple` binds `AcceptanceSpecId`, `HarnessId`, `WorkspaceId`, workspace `Generation`,
  workspace `RevisionNumber`, `PolicyId`, and `ProviderProfileId`.
- `CapabilityName` gains a verified canonical ASCII byte view and total lexicographic comparison
  whose executable result is proved equivalent to the specification order. Policy and approval do
  not depend on the ordinary derived `Ord` implementation without that contract.

`PolicyId` inside `RevisionTuple` is the sole policy identity in every B1 request and scope. B1
does not duplicate it in `CapabilityScope` or `ApprovalRequest`, and it does not add a second policy
revision number. Lease identity is exact scope plus generation/holder/claim version, so no
`LeaseId` is added. Approval already has `ApprovalRequestId`. Authority time remains in
`peritus-policy`, preserving A1’s time-independent contract.

### Shared authority time

`AuthorityInstant` contains an authority-clock `Generation` and a monotonic millisecond tick.
`ValidityWindow` is nonempty and half-open: `not_before <= now < expires_at`. Instants are ordered
only when their epochs match. Tick arithmetic is checked; it never wraps or saturates.

`AuthorityTimeState` stores the active epoch and greatest accepted tick. Every policy issuance/use,
approval request/resolution/use, and lease transition consumes a candidate observation through the
same checked reducer and stores the returned floor in its aggregate. A lower tick in the same epoch,
an epoch change without an explicit restart/reconciliation transition, or comparison across epochs
fails closed. A stateless `ValidityWindow::contains` result is never sufficient to issue authority.

Existing capabilities and approvals become unusable after an epoch change, and active leases enter
reconciliation. Wall time is diagnostic only. A2’s `FakeClock` supplies deterministic observations
in tests. C0 later owns durable epoch compare-and-swap allocation and observations; G0 must advance
the epoch on restart and complete fail-closed reconciliation before accepting authority-bearing
work.

### `peritus-policy`

#### Modules

```text
src/lib.rs
src/time.rs
src/role.rs
src/operation.rs
src/scope.rs
src/rule.rs
src/layer.rs
src/decision.rs
src/capability.rs
src/amendment.rs
src/failure.rs
src/model.rs
src/proofs/role_separation.rs
src/proofs/scope.rs
src/proofs/monotonicity.rs
src/proofs/refinement.rs
```

`ActorRole` contains stable security roles, including writer, fixer, reviewer, evaluator, gate
runner, orchestrator, evolution agent, human authority, daemon service, provider/tool worker, and
plugin. `OperationClass` contains the stable security categories used by compiled role separation.
`CapabilityName` remains the exact extensible operation name. Dots and prefixes in names have no
authority semantics.

A `Permission` is one exact resource and capability-name pair. A canonical `PermissionSet` is
nonempty, sorted, and duplicate-free. `OperationRegistry` binds each capability name to an
authenticated `OperationDescriptor` and its operation/risk classes. Evaluation rejects unknown or
inconsistent descriptors.

`CapabilityScope` binds actor, role, environment, exact permission pairs, one `RevisionTuple`,
validity window, and `UseLimit`. Its policy identity is exactly `RevisionTuple::policy_id()`.
`AuthorizationRequest` requests one complete exact scope. Whole-request behavior prevents callers
from executing an authorized subset while ignoring a denied pair.

An `AuthorityCeiling` declares the complete upper bound and immutable denies. Ceiling grants use
canonical `ScopeSelector` values. Each actor, role, environment, and permission selector is either
an exact nonempty canonical set or `AnyWithinParent`; revision selection is exact and time/use
bounds are finite intersections. `AnyWithinParent` means the containing ceiling, never the
universe. Selector containment and intersection are executable Verus functions.

Ceiling evaluation is default deny. Every requested permission pair must be covered by at least
one applicable grant whose other selector dimensions contain the request. Coverage may be the
union of several grants, but all applicable constraints participate: the effective window is the
intersection and the effective use limit is the minimum. An empty ceiling, incomplete coverage,
empty intersection, or contradictory constraint denies the whole request.

A `RestrictionLayer` contains zero or more canonical restriction rules and is not an independent
grant source. An empty restriction layer is explicitly neutral. A nonmatching rule is neutral.
A matching `RequireApproval` rule challenges the whole request, not a permission subset. All
matching approval requirements form a canonical conjunction: the responder credential must satisfy
every required authority tier, approver-role constraint, and independence predicate. An empty
intersection of approver constraints is a denial, not an approval loop. A matching deny anywhere
denies the whole request.

The effective decision order is `Denied < ApprovalRequired < Authorized`; composition takes the
minimum across the default-deny ceiling result and every complete lower restriction layer. Rule
order cannot affect the result. These no-match, coverage, conjunction, and contradiction semantics
are part of the `INV-022` theorem, not caller convention.

The public result is exactly:

```rust
pub enum PolicyDecision {
    Authorized(CapabilityIssuancePlan),
    ApprovalRequired(EscalationChallenge),
    Denied(AuthorizationDenial),
}
```

Issuance and capability-use reducers are value-in/value-out transitions. They return an opaque
logical `CapabilityIssuanceTransition` or `CapabilityUseTransition` with an exact transition digest,
not an effect permit. A use transition is bound to one `ActionId` and action digest. B0 validates
the logical transition against current kernel state and returns a verified
`KernelAuthorizationWitness`; class-V B0 never depends on or consumes C0. E0, which may depend on
both, assembles the B1 transition, B0 witness, and C0's opaque committed-event-batch receipt for the
target C1/C2/C4 authorization gateway. That lower subsystem independently validates the bundle and
alone constructs its private effect permit. No B1 or E0 bundle type by itself satisfies an effect-
worker API. Failed, forged, or indeterminate commit evidence constructs no dispatch authority.

Compiled role restrictions are non-configurable. Reviewer and evaluator exclude workspace
mutation. Writer and fixer exclude acceptance, waiver, policy amendment, and harness promotion.
Model-controlled roles exclude human-authority operations. Orchestrator transition authority does
not imply raw effect permission.

An ordinary policy amendment previews one exact `PolicyRevisionCandidate` successor of one
immutable base `PolicyId`. The candidate contains a complete checked successor policy but is not an
active-policy fact. Approval returns a separate opaque `ApprovedPolicyAmendment` whose public
accessors expose only exact matching fields/digests. B0 may expose a V-only current-kernel witness,
but it never imports or consumes the H-owned approved amendment. E0's H integration gate matches
candidate, approved amendment, kernel witness, and the first C0 receipt proving the approval
transition committed. E0 then supplies only the V-owned candidate facts and current kernel state to
B0, whose verified reducer returns a logical `PolicyActivationTransition` and exact next-state/event
plan. E0 submits that unchanged plan for a distinct second C0 commit linked to the first receipt.
Only after C0 returns the exact second receipt does E0 install/replay B0's planned state; the
replayed kernel state, not an E0-selected field, makes the new `PolicyId` active. Neither candidate,
approval, first commit, nor uncommitted B0 plan alone activates policy. The original action is then
reauthorized from the beginning against the newly active policy.

### `peritus-budget`

#### Modules

```text
src/lib.rs
src/amounts.rs
src/command.rs
src/state.rs
src/transition.rs
src/failure.rs
src/model.rs
src/proofs/conservation.rs
src/proofs/children.rs
src/proofs/observations.rs
src/proofs/refinement.rs
```

`BudgetAmounts` is a fixed five-field vector: model tokens, provider-cost microunits,
active-effect milliseconds, attempts, and retries. A map is rejected because missing/duplicate
dimensions and unstable order complicate both authority review and Verus proofs. Memory, disk,
output, process, and concurrency limits remain capacity/quota resources for scheduler and process
slices; they are not silently treated as monotonic spend.

One `BudgetLedger` owns a root and its complete child tree so child allocation and ancestor
accounting are one atomic pure transition. Every account has an immutable limit, monotonically
nondecreasing consumed vector, outstanding operation reservations, delegated child capacity, and
an open/draining/faulted/closed phase.

For every account and dimension, `child_delegated_remaining` is delegated capacity not yet
converted into descendant consumption:

```text
consumed + operation_reserved + child_delegated_remaining <= immutable_limit
available = immutable_limit - consumed - operation_reserved - child_delegated_remaining
```

Commands include child allocation, begin, activate, cumulative usage observation, exact/final
settlement, held cancellation, conservative ambiguous finalization, seal, and close. `Begin`
atomically charges `consume_now` and reserves an execution ceiling. A retry uses a fresh reservation
ID and charges attempts plus retries before execution.

These commands return logical budget transitions and receipts, not dispatch authority. E0 may
dispatch only after C0 proves that the exact `Begin` transition committed alongside the matching
capability/lease/approval inputs. C0 replay must make begin and every cumulative/final observation
idempotent by command/reservation identity. Commit ambiguity never authorizes release or a fresh
retry; it resolves the same identity or conservatively finalizes the outstanding ceiling.

Usage reports are cumulative high-water observations bound to reservation, action, and evidence
digests. A higher sample moves only its delta from reserved to consumed. A duplicate is idempotent.
A lower correction is rejected and cannot restore availability. Final settlement releases only the
unused remainder. Cancellation releases a full reservation only while held and before activation.
Once active, an indeterminate outcome conservatively consumes the entire remaining ceiling.

An observation above its ceiling consumes the still-held ceiling, records an overrun-fault receipt,
faults the account lineage, and authorizes no new work. Raw unrepresentable usage remains external
evidence rather than being wrapped, saturated, or forced into `ResourceQuantity`.

Child allocation increases the direct parent's `child_delegated_remaining` only from currently
available capacity. Every child consumption delta atomically decreases the corresponding delegated
remainder by exactly that delta at each ancestor and increases that ancestor's consumed value by
the same delta. The two changes are one transition, so a delegated unit is unavailable exactly once
and is never double-counted. Child closure requires no active reservation or child; it releases
only the unused delegated remainder while consumed amounts never move backward. Finalized
reservation records or immutable tombstones remain available to reject ID reuse and semantic replay.

### `peritus-leases`

#### Modules

```text
src/lib.rs
src/scope.rs
src/state.rs
src/claim.rs
src/command.rs
src/transition.rs
src/reconcile.rs
src/port.rs
src/failure.rs
src/model.rs
src/proofs/exclusivity.rs
src/proofs/fencing.rs
src/proofs/authority.rs
src/proofs/refinement.rs
```

A lease aggregate is keyed by `WorkspaceId` and permanently binds one resolved `ResourceId` and
`EnvironmentId`. Its phases are available, active, reconciling, quarantined, and retired. Active
state records the exact actor/session holder, generation, aggregate version, claim version,
issuance/expiry instants, and greatest accepted clock observation.

Mint establishes generation and version one. Acquire changes available to active without changing
the current generation. Renew strictly extends the deadline, advances claim version, and
invalidates the old claim. Release, expiry, holder loss, clock discontinuity, or revocation fence
the old generation before another acquisition. Generation exhaustion retires the aggregate; it
never wraps. Aggregate-version planning reserves one final representable increment exclusively for
fencing: acquire, renew, and use are rejected before they could leave an active record at the
maximum version, while the reserved fence transitions safely to retired if no later version exists.

Normal release with exact holder-quiescence evidence fences and moves directly to available in one
transition. Release without that evidence, expiry at `now >= expires_at`, holder disappearance,
clock discontinuity, and revocation fence and move to reconciling instead. Acquisition remains
blocked until an exact correlated observation says the old holder is quiescent and the resource is
safe. A correlation mismatch rejects without state change. An exactly correlated dirty or
indeterminate result may transition to quarantined. Daemon startup must reconcile every stored
active lease before mutation intake.

Mutation validation intersects the current lease with a freshly consumed exact policy capability.
The result is an opaque logical `LeaseUseTransition` bound to one action, scope, holder, generation,
and the earlier of both expiries—not an effect permit. Expired capability does not prevent voluntary
release, but no stale capability or lease can contribute to an authorized dispatch.

B1 owns the durable-CAS request and observation contract, but C0 owns its journal-backed production
implementation and the opaque `CommittedEventBatch` receipt. Requests include exact key, expected
absence or version, `CommandId`, planned next snapshot, and typed transition record. Observations
distinguish claimed-applied, conflict, definitely not applied, indeterminate, and protocol-invalid.
B1 can prove only that a claimed-applied echo exactly matches its planned successor; that validated
observation remains unprivileged and does not prove persistence. C0 must revalidate against its
authoritative transaction/idempotency record and return its own receipt. B0 supplies only a
verified current-kernel witness. E0 alone matches that witness, receipt, and logical lease
transition before constructing a private `CommittedLeaseHandle` after acquisition or
non-authoritative `MutationAuthorizationBundleClaim` for mutation. C1 independently validates the
claim and alone constructs its private mutation permit. The handle is an ergonomic holder claim,
not independent effect authority; every mutation still requires a fresh gateway-authorized permit.
Conflicts require re-observe, reauthorize, and replan; indeterminate commits are resolved under the
same command ID. A raw or merely echo-matching adapter response never constructs a lease handle or
mutation permit.

### `peritus-approval`

#### Modules

```text
src/lib.rs
src/request.rs
src/digest.rs
src/authentication.rs
src/decision.rs
src/state.rs
src/grant.rs
src/amendment.rs
src/render.rs
src/failure.rs
src/model.rs
src/proofs/replay.rs
src/proofs/binding.rs
src/proofs/refinement.rs
```

`ActionDigest`, `ApprovalRequestDigest`, and `ApprovalDecisionDigest` are distinct private-field
wrappers over `Sha256Digest`; they are not interchangeable in APIs. An approval request contains
`ApprovalRequestId`, `ActionId`, exact action digest, requester and role, escalation challenge,
structured risk inputs/digest, one `RevisionTuple`, validity window, and request digest. The sole
policy identity is `RevisionTuple::policy_id()`. Digest input covers every authority-relevant
field. Collections use the verified `CapabilityName` order and reject duplicates.

The three choices are deny, approve once, and authorize an exact policy amendment. The signed
decision payload binds a unique `CommandId`, responder, request ID/digest, choice, choice expiry,
amendment digest when present, exact `ApprovalKeyId`, credential-revocation `Generation`, and
credential-registry `RevisionNumber`. Delayed responses therefore cannot be rebound to a reissued
credential, even when the same public key is reused. The crate pins
`ed25519-dalek = { version = "=3.0.0", default-features = false }` and uses
`VerifyingKey::verify_strict`; batch, randomness, PEM/PKCS8, Serde, and legacy-compatibility
features are absent. Version 3.0.0 still compiles and exports signing APIs unconditionally, so B1
does not misrepresent it as a verifier-only dependency: an exact source-policy check rejects
`SigningKey`, signing traits/methods, and signing-key material anywhere in production B1 sources,
and the public crate API exposes only its own verifier wrapper. A public key is exactly 32 bytes, a
signature is exactly 64 bytes, and `ApprovalKeyId` is a domain-separated SHA-256 digest of the
algorithm tag and public-key bytes. Noncanonical or malformed encodings are rejected before
verification. B3 later owns external DTO/wire conversion; it cannot deserialize directly into a
verified decision.

An `ApproverCredential` binds key ID and public key to exact `ActorId`, required human-authority
role, environment and workspace scope, maximum `AuthorityTier`, allowed approver-role constraints,
validity window, and credential-revocation `Generation`. Verification consults a current immutable
`CredentialRegistrySnapshot` with an exact `RevisionNumber`; signed key ID, generation, and registry
revision must all equal it. Missing, disabled, stale-generation, not-yet-valid, expired, wrong-scope,
or under-tier credentials fail. Credentials never carry raw action permission and cannot override
an explicit policy denial.

Cryptographic verification and hashing are explicit H/TCB refinement seams. The pure state
transition consumes only a checked authentication observation whose credential, actor, payload,
scope, policy tuple, revocation generation, and validity are revalidated. Unless their executable
bodies are replaced through a separately reviewed design/PCR by Verus-verified implementations,
`ApprovalKeyId::compute` (`EXCL-0001`), `ApprovalRequestDigest::compute` (`EXCL-0002`),
`ApprovalDecisionDigest::compute` (`EXCL-0003`), and `verify_signed_decision` (`EXCL-0004`) receive
mandatory exact symbol-level exclusions. SHA-256 uses published NIST known-answer vectors; Ed25519 uses RFC 8032 vectors; both
receive alternate-implementation differential, malformed-input, and mutation/refinement evidence.
The cryptographic library's algorithm correctness is an external TCB assumption documented in the
exclusion threat analysis, not a Verus `assume` or trusted body, so `trust.toml` remains empty.

Approval states are pending, approved-once, amendment-authorized, consumed/amended, denied,
expired, and cancelled. One request has at most one non-idempotent decision. An exact repeated
signed response returns an idempotent logical result with no second state transition or action
grant; a different response after resolution fails as already resolved. Durable event uniqueness
belongs to `REF-C0-B1-COMMIT-ONCE`.

Approve-once intersects request, credential, signed response, challenge, and policy validity and
fails at equality with the earliest expiry. It produces one logical `ApprovedActionTransition`, not
an effect permit. B0 may validate V-owned request facts against current kernel state without
importing approval types; E0 matches the H approval transition, B0 witness, and C0 receipt into a
bundle claim. The target effect subsystem, not E0, independently validates that bundle and alone
constructs its private effect permit. Approve-similar produces only `ApprovedPolicyAmendment` bound
to one previewed successor. E0's two-commit integration gate is the sole future path that can submit
B0's V-only logical activation plan and install its exact replayed successor state; E0 cannot select
or invent an active policy field. New policy evaluation follows that replay.

Approver independence rejects self-approval and configured conflicting participation. UI-safe
rendering is structured and bounded: stable IDs, digests, enums, permission summaries, revisions,
validity, and redacted metadata. It never emits raw action payload, secret values, terminal control
bytes, unbounded reason text, or arbitrary debug representations.

### Formal model and proof ownership

Ordinary `state_machine!` models transition reachability. Executable reducers use deterministic
sorted vectors and checked arithmetic and prove refinement to mathematical sets/maps/trees. Rust
move semantics enforce non-clone logical transition grants; formal state proves that only one
matching logical transition is reachable. C0/E0 refinements, not B1, lift that claim to durable
commit and effect-permit uniqueness.

| Obligation | Owner | Required statement |
|---|---|---|
| `INV-006` | `peritus-leases` | At most one logical active lease record exists for a workspace generation; fencing prevents stale claim revival. |
| `INV-007` | `peritus-policy` | Every issuable scope satisfies immutable actor-role operation separation. |
| `INV-008` | `peritus-policy` | Issuance/use preserves exact scope dimensions and decrements limited uses exactly once. |
| `INV-009` | `peritus-approval` | One request has one semantic resolution and one exact approve-once consumption. |
| `INV-012` | `peritus-budget` | Consumption is monotonic; reservations/delegation conserve immutable capacity; retries cannot bypass exhaustion. |
| `INV-022` | `peritus-policy` | Ordinary layer composition cannot broaden; explicit amendment stays within higher ceilings. |
| `POLICY-DENY-WINS` | `peritus-policy` | An applicable deny prevents capability issuance for the whole request. |
| `POLICY-PAIR-SCOPE` | `peritus-policy` | Exact permission pairs cannot create Cartesian authority. |
| `POLICY-AMENDMENT-CEILING` | `peritus-policy` | Successor policy never escapes higher-tier ceiling or immutable denial. |
| `AUTHORITY-TIME-MONOTONIC` | `peritus-policy` | Accepted authority observations remain in one epoch with nondecreasing ticks; discontinuity grants no logical authority. |
| `BUDGET-CONSERVATION` | `peritus-budget` | Consumed, reserved, and delegated quantities never exceed immutable limits. |
| `BUDGET-OBS-HIGH-WATER` | `peritus-budget` | Accepted cumulative observations never decrease or charge twice. |
| `BUDGET-CHILD-SUBSET` | `peritus-budget` | Children cannot mint capacity and consumption reaches every ancestor. |
| `LEASE-FENCING` | `peritus-leases` | Every takeover path invalidates the prior generation before availability. |
| `LEASE-CAPABILITY-INTERSECTION` | `peritus-leases` | Logical lease-use transition is no broader than both exact current logical authorities. |
| `APPROVAL-DIGEST-BINDING` | `peritus-approval` | Logical decision/use transition matches the exact request/action/policy/revision digests. |
| `APPROVAL-TERMINALITY` | `peritus-approval` | Terminal decisions cannot transition to a second semantic result. |

The reviewed proof dependency DAG is:

```text
POLICY-PAIR-SCOPE ─┬──> INV-008 ────────────────> INV-009
INV-007 ───────────┤          └──> LEASE-CAPABILITY-INTERSECTION
AUTHORITY-TIME-MONOTONIC ─────┴───────────────┬──> INV-009
                                              └──> INV-006
POLICY-DENY-WINS ────────────────> INV-022
POLICY-AMENDMENT-CEILING ────────> INV-022
APPROVAL-DIGEST-BINDING ─────────> INV-009
APPROVAL-TERMINALITY ────────────> INV-009
BUDGET-CONSERVATION ─┬───────────> INV-012
BUDGET-OBS-HIGH-WATER┤
BUDGET-CHILD-SUBSET ─┘
LEASE-FENCING ───────────────────> INV-006
```

Separate stable downstream open refinements prevent B1 from overclaiming integrated behavior:

| ID | Future owner | Required refinement |
|---|---|---|
| `REF-C0-B1-COMMIT-ONCE` | C0 | Transactional append/CAS and opaque commit receipts preserve capability, budget begin/reservation/usage/finalization, lease, and approval transitions under crash/replay without lost/double consumption or unsafe release. |
| `REF-B3-B1-DIGEST-BYTES` | B3 | Canonical action/event bytes match every digest consumed by B1. |
| `REF-C1-B1-RESOURCE-IDENTITY` | C1 | Resolved filesystem/workspace targets match exact authorized `ResourceId` values. |
| `REF-C1-B1-RECONCILE-SAFETY` | C1 | Exactly correlated post-fence workspace inspection establishes safe-to-acquire or a typed dirty/indeterminate result without guessing. |
| `REF-C2-B1-HOLDER-QUIESCENCE` | C2 | Process/task ownership evidence proves the exact prior lease holder is quiescent before normal release or safe reconciliation. |
| `REF-C4-B1-OPERATION-CLASS` | C4 | Each concrete tool consumes B1 `OperationDescriptor` values and its implementation matches the authenticated operation class. |
| `REF-B0-B1-CURRENT-STATE-WITNESS` | B0 | V-only kernel logic binds current lifecycle/policy/revision/action facts and produces the logical `PolicyActivationTransition`/next-state plan from V-owned candidate facts without importing H approval or C0 types. |
| `REF-E0-B1-COMMIT-BEFORE-EFFECT` | E0 | E0 assembles matching capability use, durable budget charge/reservation, lease use when mutating, approval when required, B0 current-state witness, and exact C0 receipts before invoking a target authorization gateway; it does not construct that target's permit. |
| `REF-C1-B1-AUTHORITY-GATE` | C1 | The workspace crate owns `WorkspaceAuthorizationRequest`; its gateway validates E0-supplied B1/B0/C0 inputs and alone constructs its private mutation permit, with no public raw bypass. |
| `REF-C2-B1-AUTHORITY-GATE` | C2 | The process crate owns `ExecutionAuthorizationRequest`; its gateway validates E0-supplied B1/B0/C0 inputs and alone constructs its private execution permit, with no public raw bypass. |
| `REF-C4-B1-AUTHORITY-GATE` | C4 | The tool router owns `ToolAuthorizationRequest`; it validates E0-supplied B1/B0/C0 inputs and alone constructs its private invocation permit, with no built-in-tool bypass. |
| `REF-E0-B1-POLICY-ACTIVATION` | E0 | E0 matches candidate, H-owned approved amendment, B0 witness, and first C0 approval receipt; submits B0's unchanged V-only activation plan for a distinct linked C0 commit; then installs/replays only B0's exact next state after the second receipt. |
| `REF-B2-B1-CONTRACT-REVISION` | B2 | Acceptance-contract validation binds every authority request to one exact `RevisionTuple`. |
| `REF-B0-B1-REVISION-FRESHNESS` | B0 | Kernel lifecycle checks reject B1 transitions whose exact `RevisionTuple` is not current. |
| `REF-B0-B1-BUDGET-CEILING` | B0 | Kernel integration proves root limit is within the B2 effective policy ceiling and child limit is within parent availability. |
| `REF-C0-B1-CLOCK-EPOCH` | C0 | Durable compare-and-swap allocation never reuses or decreases the authority-clock epoch and returns bounded observations. |
| `REF-G0-B1-STARTUP-FENCING` | G0 | Restart advances the durable epoch, fences/reconciles every active lease, invalidates prior capability/approval time state, and blocks mutation intake until complete. |

These IDs are reserved in the architecture/design traceability registry when B1 lands. Each future
owner adds a real `verification/obligations.toml` entry only with its registered package, exact
source symbol, live issue, and registered actor; placeholders are invalid. None is counted as a B1
discharge.

### Error model

Each crate exposes a closed error family with a stable `PERITUS-POLICY-*`, `PERITUS-BUDGET-*`,
`PERITUS-LEASE-*`, or `PERITUS-APPROVAL-*` code and a recovery classification of terminal,
reobserve, reauthorize, resolve-indeterminate, or caller-correctable. Errors retain relevant typed
IDs and exact mismatch category but never include secret/action payload bytes.

Expected categories include unknown/stale identity, duplicate-ID conflict, invalid canonical
input, scope mismatch per dimension, role denial, explicit denial, approval required, exhausted
use, not-yet-valid/expired/clock discontinuity, arithmetic overflow/underflow, insufficient budget
with exact limiting dimensions, nonmonotonic usage, overrun fault, illegal state transition,
generation/version exhaustion, reconciliation mismatch, CAS conflict/indeterminate/protocol
violation, authentication failure, self/conflicted approval, replay, and unsafe rendering input.

### Parallel implementation boundaries

The implementation is divided into conflict-free lanes after the shared contract commit is frozen:

| Lane | Exclusive paths | Inputs it may consume | Paths it may not edit |
|---|---|---|---|
| Shared foundation/PCR | `peritus-types`, root workspace, architecture, verification governance, `xtask` macro policy | Frozen B1 design | Four B1 crate implementations |
| Policy | `crates/foundation/peritus-policy` | A1 additions | Budget, leases, approval, shared manifests |
| Budget | `crates/foundation/peritus-budget` | A1 additions | Policy, leases, approval, shared manifests |
| Leases | `crates/state/peritus-leases` | Frozen policy permit/time APIs | Policy internals, budget, approval, shared manifests |
| Approval | `crates/state/peritus-approval` | Frozen policy escalation/amendment/time APIs | Policy internals, budget, leases, shared manifests |
| Integration | root/architecture/verification/docs/A2 exhaustive fixtures | Reviewed crate commits | Crate internals except review fixes coordinated with owner |

Policy and budget can run in parallel after shared types freeze. Leases and approval can then run in
parallel against the compiled policy contract. Integration owns no silent API redesign. Any needed
cross-crate change returns to the contract owner and updates the design/PCR.

## Data and compatibility

B1 domain crates do not depend on Serde and do not establish the external protocol. B3 will define
versioned DTOs and canonical persisted/event bytes. B1 nevertheless freezes semantic field sets,
ordering rules, error codes, state transitions, and privilege-construction rules now.

Privileged state is reconstructed only by checked initialization and replay of accepted
transitions. Raw state hydration into a capability, permit, active lease, approved state, budget
closure, or policy successor is forbidden. Digest wrappers accept exact fixed-size bytes and do not
claim that those bytes describe the eventual action until the B3/C4 refinement is discharged.

B1 supplies internal minimal, realistic, corrupt, and adversarial semantic fixtures. B3 converts
them into external compatibility fixtures without weakening validation. Named resource dimensions
are used on the wire; enum ordinals and floating-point cost values are prohibited.

## Failure and recovery behavior

- Policy denial and approval-required are domain outcomes, not adapter failures.
- Any policy/revision/time mismatch invalidates the pending authority and requires a new decision.
- A failed pure transition returns no next state. An accepted transition returns a next-state plan
  that grants no effect authority until exact durable commit is observed.
- A budget effect that may have executed is never retried by releasing its reservation. It is
  resolved from cumulative evidence or conservatively charged to the ceiling.
- Lease conflict reobserves and replans. Lease commit indeterminacy resolves the same `CommandId`;
  it never constructs a committed handle or retries under a new identity.
- Expired/disappeared lease holders fence immediately and reconcile before availability. Unsafe or
  unknown resource state quarantines rather than guessing.
- Duplicate identical approval responses are idempotent. Conflicting responses, stale requests,
  and already-consumed permits are terminal for that request.
- Arithmetic exhaustion, version exhaustion, corrupt replay state, invalid adapter echoes, and
  impossible state combinations fail closed and surface stable diagnostics.

## Security analysis

The model, repository instructions, external content, providers, tools, and raw adapter responses
are untrusted inputs. B1 validates them into logical transition evidence only; none alone satisfies
an effect-worker authority type. Exact permission pairs prevent accidental cross-products. Compiled
role restrictions prevent configuration from giving a writer acceptance authority or a reviewer
mutation authority. Deny dominance prevents approval from becoming a universal bypass. Immutable
policy IDs and exact revision tuples prevent stale reuse.

Authority time is monotonic and epoch-bound, avoiding wall-clock rollback attacks. Lease fencing
prevents ABA and stale-holder revival. CAS observations are evidence to validate, not proof of
commit; C0's journal transaction and commit receipt are an explicit effect/TCB refinement boundary.
Budget high-water accounting prevents duplicate, reordered, refunded, and ambiguous provider
reports from reminting spend. Approval signatures bind exact fixed content, self/conflict checks
enforce independence, and structured rendering prevents untrusted payloads from becoming a
terminal/UI injection surface.

Cryptographic algorithms, production authority-clock observations/epoch allocation, the durable
store/CAS implementation, canonical action encoding, resolved resource identity, and concrete tool
classification are explicit effect or TCB refinement boundaries. B1 proves fail-closed validation
of their bounded observations, not their external truth. Their claims are not silently folded into
a pure Verus theorem.

## Verification plan

1. Add independent reference models for all four reducers. They must not import production decision
   helpers.
2. Exhaustively enumerate small policy scopes, budgets, lease traces, and approval traces and compare
   accepted/rejected results with production reducers after every step.
3. Run larger deterministic generated traces with seed and minimal failing trace included in
   diagnostics.
4. Exercise exact zero, one, boundary, one-over, and `u64::MAX` arithmetic paths.
5. Test every scope field by holding all other fields equal and changing exactly one.
6. Add adversarial port scripts for before-commit, after-commit/before-ack, conflict,
   indeterminate, malformed echo, stale substitution, and corrupt reconstruction.
7. Add concurrency-model tests for competing lease acquisition/renew/expiry/reconcile and approval
   consume. Exactly one logical transition may win; a raw lease CAS claim remains unprivileged.
8. Add mutation-sensitive tests for deny guards, scope pairing, role separation, budget high-water,
   delegation propagation, lease fencing, approval binding, and terminality.
9. Confirm private constructors and dependency/API checks prevent a B1 logical transition from
   being treated as an effect permit. Publish deterministic forged, malformed, stale, conflicting,
   duplicate, and indeterminate commit-claim fixtures for capability, budget, lease, and approval
   transitions; their integrated rejection tests are mandatory evidence for the named C0/E0/C1/C2/
   C4 refinements. Within B1, ambiguous budget state never authorizes release or a fresh identity.
10. Run NIST SHA-256 and RFC 8032 Ed25519 vectors, alternate-implementation differential cases,
    invalid/malleated encoding cases, credential revocation/scope/tier cases, delayed responses
    after same-key reissue, and exact expiry equality. Exercise rendering with control bytes, invalid UTF-8 source observations, every
    truncation edge, seeded secret canaries, and bounded worst-case collections.
11. Verify each class-V and class-H package with `--no-cheating` and record exact commands in proof
   evidence.
12. Run the complete Gate A locally, then on the implementation PR, then on exact merged `main`.

## Rollout

Rollout means internal integration, not an MVP or public release.

1. Freeze and independently approve this design.
2. Implement the exact shared A1 additions and narrow `state_machine!` checker support with
   adversarial checker tests.
3. Implement policy and budget in parallel against the frozen A1 contract.
4. Implement leases and approval in parallel against the frozen policy APIs.
5. Integrate root manifests, architecture registry, formal inventories, docs, and A2 exhaustive
   fixtures; run focused and full verification.
6. Obtain an independent code/proof/security review and resolve findings at their causes.
7. Compute exact final formal/shared input fingerprints. Create a proof-authorization branch from
   current protected `main` containing only the append-only PCR record, review it independently,
   and merge its PR.
8. Rebase/merge protected `main` into the B1 implementation branch, update the current source
   inventory to point at the preauthorized fingerprints without changing them, rerun Gate A, and
   merge the implementation through its protected PR.
9. Verify the exact resulting `main` commit and record all CI evidence before closing B1.

Rollback before public release is a normal protected revert of the complete B1 integration commit
plus a proof-impact record for the reverted exact identities. Proof history remains append-only;
the PCR authorization is never deleted or rewritten.

## Open questions

No user decision is currently required. The following are downstream contracts, not B1 blockers:

- C0 selects and implements the durable journal/CAS technology while satisfying B1 ports.
- B3 selects external protocol encodings while preserving B1 semantic validation.
- C1 defines resolved workspace/resource identity and proves it matches `ResourceId`.
- C4 registers concrete operation descriptors and proves classifications match implementations.
- B0/B2 define integrated lifecycle/acceptance transitions around the shared `RevisionTuple`.

If pinned Verus rejects a specific deterministic construct during implementation, the owner first
redesigns it into supported Verus Rust. A proof exclusion is a reviewed last resort and cannot be
used to move an authority decision wholesale into ordinary Rust.

## Out of scope

The following work belongs to later slices and is deliberately not duplicated in B1:

- B0 session/run/action lifecycle and acceptance transitions;
- B2 acceptance-contract and gate/reviewer policy definitions;
- B3 public command/event schemas, canonical general-purpose codec, and compatibility migrations;
- C0 production journal, projection, transactional CAS, and restart implementation;
- C1 filesystem/Git target resolution, workspace mutation, snapshots, and rollback;
- C2/C3 process and platform resource enforcement;
- C4 concrete tool implementations and operation-classification refinement;
- daemon clock-epoch persistence and startup orchestration;
- public CLI/TUI/API approval presentation.

These are not optional capabilities or deferred quality. B1 provides their frozen authority
contracts and records the exact refinements they must discharge before production release.

## Architecture alternatives and verdict

### Alternative: one authority crate

Merging policy, budget, leases, and approval would simplify access to private internals, but it
would mix pure policy algebra, monotonic accounting, state/persistence ports, cryptography, and UI
rendering. It would also serialize parallel work and create the god-crate boundary this architecture
is intended to avoid.

### Alternative: approval inside policy

This would shorten a few proof paths, but cryptography and rendering would contaminate the class-V
policy dependency base. Keeping approval as an H consumer preserves a pure reusable policy core.

### Alternative: wall-clock authority and expiry-to-available leases

Wall clocks can regress or jump. Expiry alone does not prove an old holder has stopped mutating.
Both approaches can revive stale authority and are rejected.

### Alternative: refundable or close-settled budgets

Decreasing consumption or delaying ancestor accounting until child close can remint capacity after
provider correction, crash, or retry. High-water cumulative settlement with immediate ancestor
propagation is preferred.

### Alternative: `tokenized_state_machine!`

Linear ghost ownership is attractive, but the pinned macro documents a trusted generated
implementation. B1 uses ordinary `state_machine!`, executable reducers, move-only Rust values, and
explicit refinement proofs instead.

### Final verdict

Proceed with the four-crate design above. It is the narrowest dependency structure that still owns
the complete production B1 authority semantics, maximizes feasible Verus coverage, exposes honest
effect boundaries, and permits policy/budget and lease/approval implementation lanes to run in
parallel without editing one another’s paths.
