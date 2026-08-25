# Feature: D2 Production Review Engine

## Summary

D2 adds `crates/orchestration/peritus-review` as the durable, deterministic review and finding
lifecycle boundary for Peritus. The crate consumes immutable B2 acceptance contracts and quality
observations, C6 reviewer-context and independence facts, and C0 journal/checkpoint services. It
accepts inert structured review and fixer records, applies a closed reducer, and projects current
review, finding, and externally authorized waiver observations for B2. It never invokes a model or
provider, runs a tool, mutates a workspace, grants a waiver, approves a run, or declares overall
acceptance.

The aggregate is run-scoped and retains every review cycle across candidate revisions. Each
reviewer assignment/submission has one stable `ReviewCycleId` and one one-based
`ReviewCycleOrdinal`, matching the frozen B2 `ReviewObservation` API. A cycle binds one immutable
contract identity/digest and review-policy snapshot, exact `RevisionTuple`, candidate digest, tree
digest, reviewer identity, assigned categories, and C6 fresh-context/independence facts. A change
to any revision-tuple component or candidate/tree digest makes prior review, finding, resolution,
and waiver evidence historical rather than current; no history is deleted.

The architecture verdict is **ready**. Required stable tags, authority boundaries, migration
shape, state transitions, parallel ownership, and verification evidence are fixed below.

## User-visible behavior

1. Later orchestration creates a review aggregate for a run from a checked
   `AcceptanceContract`, exact candidate revision, and explicit D2 bounds.
2. Reviewers are assigned deterministic cycles with declared categories and C6-attested fresh,
   read-only, producer-independent context facts.
3. A reviewer submits one bounded structured record. The submission either validates completely
   and becomes durable or fails with a typed error; partial or malformed output never counts.
4. Findings receive stable identities and retain category, severity, blocking status, confidence,
   requirements, locations, evidence, reproduction, expected behavior, remediation, exact
   revision, every source reviewer, and complete disposition history.
5. Duplicate findings may be reconciled under one canonical identity. The duplicate identities,
   reviewer sources, evidence references, and disposition history remain traceable.
6. A fixer records `Fixed`, `Disputed`, `SupersessionProposed`, or `WaiverRequested` with current
   evidence. Only a current-revision reviewer confirmation closes a fix, invalidation, or
   supersession. A waiver closes a blocker only after D2 consumes an exact external B1/B2 waiver
   observation authorized for that finding and revision.
7. Quorum reports each B2 dimension separately: submitted-review count, required-category
   coverage, distinct reviewer identities, producer independence, distinct contexts, distinct
   model families, distinct providers, and no shared ancestry. C6 fresh-context validity is also
   reported and required.
8. Finalization succeeds only as D2 review completion: all quorum dimensions pass and every
   current finding is conserved by a reviewer-confirmed resolution/invalidation/supersession or
   externally authorized waiver. This result is evidence for B0/E0, not overall run acceptance.
9. Repeated finding sets, non-improving/regressing severity, review-cycle exhaustion, or explicit
   budget exhaustion terminate truthfully as `NeedsHuman` or `Failed`. Cancellation and malformed
   submissions never become success.
10. Restart loads canonical events from C0, replays from genesis, and requires the installed D2
    checkpoint to match every state field and digest.

## Requirements

### Binding and bounded inputs

- **D2-R001:** `ReviewBinding::from_contract` shall call `AcceptanceContract::bind`, copy the
  complete checked B2 review policy and maximum-review-cycle limit, and bind the exact
  `RevisionTuple`, candidate digest, tree digest, and producer ancestry/identity set.
- **D2-R002:** A review cycle shall contain its stable ID/ordinal, immutable binding digest,
  reviewer identity, assigned canonical categories, C6 context-plan digest, fresh-context fact,
  and the complete B2/C6 independence view.
- **D2-R003:** `ReviewLimits` shall independently bound cycles, assignments, submissions,
  findings, categories, requirements, locations, evidence references, provenance sources,
  disposition records, path/text/opaque bytes, and total encoded payload/state bytes. Checked
  constructors shall reject zero, overflow, noncanonical order, duplicates, or values above the
  compiled production ceilings before allocation or durable admission.
- **D2-R004:** Closed enums and protocol tags shall reject unknown values. Required text fields
  shall be nonempty valid UTF-8 within their individual bounds. Source locations shall use bounded
  repository-relative UTF-8 paths and valid nonzero inclusive line/column ranges.

### Review and finding lifecycle

- **D2-R010:** The closed command vocabulary shall cover genesis, revision advance/invalidation,
  reviewer assignment, structured submission, duplicate reconciliation, fixer response,
  reviewer resolution/invalidation/supersession confirmation, waiver request, external waiver
  observation, cancellation, failure, budget exhaustion, and deterministic finalization.
- **D2-R011:** Each accepted command shall emit exactly one event and successor state. Commands
  shall bind `CommandId`, `EventId`, run identity, exact revision, expected sequence, expected
  predecessor event, prior state digest, and command kind. Stale fences or command-identity reuse
  with different bytes shall fail without a transition.
- **D2-R012:** A reviewer assignment shall use a unique cycle identity/ordinal and may submit at
  most once. Submitted categories must be a nonempty canonical subset of the assignment and the
  contract declarations. Unknown, duplicate, or contradictory categories/findings are rejected.
- **D2-R013:** `Finding` shall retain stable ID, originating cycle/reviewer, all reconciled source
  cycles/reviewers, category, severity, derived blocking flag, bounded confidence, canonical
  requirement IDs, bounded locations, evidence references, description, reproduction, expected
  behavior, remediation, affected revision, normalized digest, and complete append-only
  disposition history.
- **D2-R014:** A finding starts open. A fixer response is evidence, not closure. `Fixed` and
  `Disputed` remain open pending reviewer action; `WaiverRequested` remains open pending external
  authority; and proposed supersession remains open until reviewer confirmation.
- **D2-R015:** A current finding is conserved until exactly one current disposition is established:
  reviewer-confirmed resolution, reviewer-confirmed invalidation, provenance-preserving confirmed
  supersession, or a current external authorized waiver. No other event removes it from the open
  set.
- **D2-R016:** Duplicate reconciliation shall select an existing canonical finding, absorb every
  duplicate's source cycles/reviewers/evidence/history, and mark each duplicate as superseded by
  the canonical identity. It shall reject cycles, self-supersession, already conflicting
  supersession, category/revision mismatch, or loss of provenance.
- **D2-R017:** Reviewer confirmation shall come from an assigned reviewer whose identity is
  independent from the fixer/producer as required, name the current finding revision and pending
  response, and provide bounded evidence. Contradictory or stale confirmations are rejected.
- **D2-R018:** D2 shall accept only an existing `WaiverObservation` matching a previously requested
  finding, current revision, contract waiver declaration, approval request, authority reference,
  evidence requirement, and waiver digest. It shall never construct or authorize that observation.

### Quorum, freshness, and truthful termination

- **D2-R020:** `QuorumReport` shall evaluate required-category coverage, total current submitted
  review count, distinct reviewers, producer independence, distinct context digests, distinct
  model families, distinct providers, no shared ancestry, and C6 fresh context as independent
  named results. No composite shortcut may hide a failed dimension.
- **D2-R021:** Only completely validated, current-revision submissions count. Assignments,
  malformed attempts, cancelled cycles, missing submissions, or stale records do not count.
- **D2-R022:** `AdvanceRevision` shall require a checked new binding. When any component of the
  `RevisionTuple`, candidate digest, or tree digest differs, all earlier cycles, findings,
  resolution confirmations, and waiver observations remain historical but are excluded from every
  current projection and quorum/conservation decision.
- **D2-R023:** Current B2 projections shall contain canonical `ReviewObservation`,
  `FindingObservation`, and `WaiverObservation` values. Confirmed resolved findings map to
  `FindingDisposition::Resolved`; open/fixed/disputed findings map to `Open`; waiver requests map
  to `WaiverRequested`; invalidated or duplicate-superseded identities are omitted while their
  provenance remains under the retained state/canonical finding.
- **D2-R024:** Repeated canonical finding fingerprints across cycles, flat or worsening maximum
  severity across configured consecutive cycles, the contract maximum review cycles, and explicit
  budget exhaustion shall be deterministic terminal inputs. Repetition/stagnation/disagreement
  yields `NeedsHuman`; corrupt or impossible state and explicitly unrecoverable failure yield
  `Failed`.
- **D2-R025:** `ReviewTerminalKind` shall be closed over `Completed`, `NeedsHuman`, `Failed`, and
  `Cancelled`. Only `Completed` requires current quorum plus finding conservation. No failure,
  cancellation, budget exhaustion, malformed input, missing evidence, disagreement, or limit
  exhaustion path may produce `Completed`.

### Durability, protocol, and compatibility

- **D2-R030:** D2 shall implement canonical schema-v1 command, event, and complete-state frames in
  B3 families 53 (`review-command`), 54 (`review-event`), and 55 (`review-state`). Decoded values
  are inert and must pass D2 constructors/reducer checks before becoming authoritative.
- **D2-R031:** C0 shall add `AggregateKind::Review` with immutable stable tag 9. Existing tags 1–8
  and existing event bytes shall retain their exact meaning.
- **D2-R032:** D2 shall use a dedicated checkpoint namespace `0xD201` and a domain-separated
  run-state key. A journal append shall atomically commit one family-54 event and install its
  complete family-55 successor checkpoint using C0 head and state CAS.
- **D2-R033:** Exact command replay shall return the already committed event/checkpoint; the same
  `CommandId` with different canonical command bytes shall conflict. A checkpoint ahead of,
  behind, absent from, or different from semantic replay shall fail closed.
- **D2-R034:** Replay from genesis shall reject gaps, duplicate event IDs, predecessor mismatch,
  revision mismatch, unknown tags, state-digest mismatch, illegal semantic transitions, and
  trailing bytes. Replayed state shall byte-for-byte match canonical live state.
- **D2-R035:** C0 migration version four shall widen only the closed aggregate-kind checks from
  1–8 to 1–9 by table-copy migration with row-count and metadata checks. It shall require a
  completed backup, preserve historical rows/bytes/order, update both journal and migration schema
  versions, and retain tested restore/old-binary read behavior. Exact pre-D2 schema fixtures shall
  prove tags 1–8 and families already stored remain unchanged.
- **D2-R036:** B3 registry, schema JSON, TypeScript constants/types, SHA256 manifests, canonical
  binary fixtures, architecture ownership, controlled-root inventory, reproducibility checks, and
  A2 D2 conformance catalog shall be updated from canonical generators.

### Verus and maintainability

- **D2-R040:** Binding validity, bounds, legal disposition transitions, reducer fences, exact
  freshness, quorum dimensions, finding conservation, terminal truth, oscillation limits, and
  replay equivalence shall be executable Verus Rust with explicit spec predicates and proof
  functions where the toolchain supports the used data.
- **D2-R041:** The crate shall use no state-machine macro, `assume`, `admit`, axiom,
  `external_body`, unsafe code, proof-only hidden caller precondition, placeholder, ignored test,
  or unnecessary architecture/source exception.
- **D2-R042:** The ordinary Rust API shall execute the same checked bodies after ghost erasure and
  return typed errors. Public fields remain private. `lib.rs` remains a composition/export surface
  below 80 lines; production Rust files target 400 lines and never exceed 700.
- **D2-R043:** No production dependency may offer provider execution, process/shell execution,
  workspace mutation, approval issuance, or overall acceptance authority.

## Acceptance criteria

| Criterion | Required evidence |
|---|---|
| Binding and strict structured validation | constructor matrix for every missing/malformed/duplicate/stale/bounded field and one-field-at-a-time tuple drift |
| Complete legal lifecycle | reference model/property traces covering every command and first illegal transition |
| Quorum dimensions stay independent | one positive and one isolated negative test for count, categories, reviewer identity, producer relation, context, model family, provider, ancestry, and fresh context |
| Finding conservation | property tests proving no open current finding disappears without one permitted current closure |
| Reconciliation retains provenance | multi-review duplicate fixture retaining all source reviewers, cycles, evidence, and histories |
| Resolution and waiver authority | fixed/disputed/confirmed/stale/contradictory cases and external authorized/denied/mismatched waiver cases |
| Truthful terminal behavior | completion, cancellation, failure, budget exhaustion, max cycles, repeat set, stagnation/regression, disagreement, and missing evidence cases |
| Durable replay | real SQLite commit/restart/idempotency/conflict/corrupt checkpoint/genesis replay tests |
| Stored compatibility | version-three fixture backup/migrate/integrity/restore test plus historical aggregate tags 1–8 and frame bytes unchanged |
| Protocol reproducibility | exact family 53–55 frames, checked generated JSON/TypeScript/SHA256 outputs, unknown-tag and bounds tests |
| A2 conformance | nonempty lifecycle, quorum, independence, reconciliation, stale revision, resolution, waiver, restart, oscillation, and malformed catalogs |
| Formal evidence | focused no-cheating Verus verification of the D2 proof roots and ordinary-API audit |
| Repository quality | focused test/Clippy/rustdoc, architecture/generated/inventory checks, full serialized Gate A, signed PR, green hosted matrices, signed merge, and fresh-main revalidation |

## Current architecture

B0 already owns the overall run lifecycle, including coarse review and waiver phases, and remains
the only kernel that can later accept a run. B2 owns immutable `AcceptanceContract`,
`ReviewPolicy`, `ReviewerIndependence`, `ReviewObservation`, `FindingObservation`, and external
`WaiverObservation`; its evaluator already checks exact revisions and all six independence flags.
C6 owns reviewer role policy, mandatory fresh/read-only context behavior, context-plan identities,
and an exact `ReviewIndependenceView`. C0 owns `SqliteJournal`, atomic event plus state installation,
command idempotency, replay records, and backup-required forward migrations. D1 demonstrates the
accepted durability pattern using protocol families 50–52, `AggregateKind::Gate` tag 7, and a
dedicated checkpoint namespace. C7 currently uses aggregate tag 8, so tag 9 is the next stable
value. B3 families 53–55 are the free ordered range between D1 and trace family 60.

The crate belongs to orchestration owner D2 and verification class H: its deterministic domain
core is V/Verus, while canonical SHA-256 encoding and SQLite persistence are narrow existing
H-class boundaries. Its production dependencies are limited to `peritus-codec`,
`peritus-protocol`, `peritus-spec`, `peritus-quality-policy`, `peritus-role`, `peritus-context`,
`peritus-journal`, `peritus-evidence`, `peritus-types`, and `vstd` as justified by the individual
modules.

## Proposed design

### Run aggregate and data flow

`ReviewRunState` is the sole D2 authority. It stores the run ID, current binding, phase, sequence,
last event, state digest, canonical historical cycles, canonical findings, observed external
waivers, current quorum/oscillation summaries, consumed command identities where needed for pure
replay, and terminal state. Collections have deterministic identity order; append-only history
records preserve event order explicitly.

`ReviewBinding` stores the B2 `ContractBinding`, copied review policy, maximum cycles, candidate
and tree digests, producer actor/ancestry identities, and a canonical binding digest. The
constructor takes `&AcceptanceContract`; raw fields cannot manufacture a checked binding.

`ReviewCycle` moves through `Assigned`, `Submitted`, `Cancelled`, or `Invalidated`. The assignment
stores `ReviewerIdentity`, C6 plan digest/fresh-context fact, and categories. `ReviewSubmission`
owns the validated structured records. Rejected submission parsing occurs before reduction and
does not create a review event. An explicit failure command may durably record that an external
orchestration attempt failed, but it cannot count as a review.

`FindingState` owns an immutable normalized body plus `FindingSource` provenance records and
append-only `DispositionRecord` values. `CurrentDisposition` is derived from history rather than
independently writable. Superseded duplicates remain addressable historical records, while the
canonical finding carries absorbed provenance.

`QualityProjection` converts only current, fully validated cycles and current canonical findings
into B2 observations. Projection conversion calls B2 constructors, preserving B2 canonical-order
and revision checks rather than duplicating or bypassing them.

### Closed commands, events, and reducer

Commands and events use parallel exhaustive variants:

```text
StartRun
AdvanceRevision
AssignReviewer
SubmitReview
ReconcileDuplicates
RecordFixerResponse
ConfirmResolution
ConfirmInvalidation
ConfirmSupersession
RequestWaiver
ObserveWaiver
CancelCycle
CancelRun
ExhaustBudget
FailRun
FinalizeRun
```

`start` accepts only genesis `StartRun`. `decide` validates plan/run/revision/sequence/predecessor/
state-digest fences, applies exactly one semantic transition to a clone, recomputes derived quorum,
conservation, and oscillation summaries, advances the event cursor once, and hashes the canonical
successor. `replay` reconstructs the corresponding command from each event and requires the newly
reduced event to equal the stored event exactly.

`FinalizeRun` computes terminal truth rather than accepting a caller-supplied result. It produces
`Completed` only if the current binding has sufficient submitted reviews, all required categories
and enabled independence facts pass, C6 fresh-context facts pass, no unconserved current finding
exists, no oscillation/limit trigger exists, and the run is active. Otherwise it returns a typed
rejection or, for an already established exhaustion/escalation condition, the corresponding
truthful non-success terminal.

### Module layout and frozen ownership

```text
crates/orchestration/peritus-review/
  Cargo.toml                         # root integrator
  README.md                          # root integrator
  src/
    lib.rs                           # root integrator
    binding.rs                       # core/formal worker
    limits.rs                        # core/formal worker
    reviewer.rs                      # core/formal worker
    finding.rs                       # core/formal worker
    finding/location.rs              # core/formal worker
    disposition.rs                   # core/formal worker
    waiver.rs                        # core/formal worker
    command.rs                       # core/formal worker
    event.rs                         # core/formal worker
    state.rs                         # core/formal worker
    state/mutation.rs                # core/formal worker
    reducer.rs                       # core/formal worker
    reducer/apply.rs                 # core/formal worker
    quorum.rs                        # core/formal worker
    reconciliation.rs                # core/formal worker
    oscillation.rs                   # core/formal worker
    observation.rs                   # core/formal worker
    verified.rs                      # core/formal worker
    error.rs                         # core/formal worker
    canonical.rs                     # codec/durability worker
    wire/mod.rs                      # codec/durability worker
    wire/command.rs                  # codec/durability worker
    wire/event.rs                    # codec/durability worker
    wire/state.rs                    # codec/durability worker
    durability.rs                    # codec/durability worker
    durability/binding.rs            # codec/durability worker
    replay.rs                        # codec/durability worker
    projection.rs                    # codec/durability worker
  tests/
    domain_*.rs                      # core/formal worker
    codec_*.rs                       # codec/durability worker
    durability_*.rs                  # codec/durability worker
    replay_*.rs                      # codec/durability worker
```

The root integrator exclusively owns workspace manifests/lockfile, `architecture.toml`, all B3
registry/generator/fixture edits outside this crate, C0 aggregate and migration edits, A2 catalogs,
generated artifacts, repository docs/inventories, Git operations, and every Cargo/Clippy/rustdoc/
Verus/Gate-A command. Workers do not edit shared files and do not run workspace-wide verification.

### Durability and migrations

`commit_review_transition` follows D1's exact event-plus-checkpoint pattern. It encodes the
family-53 command, family-54 event, and family-55 successor state; resolves command idempotency;
compares the D2 aggregate head and namespace `0xD201` state record; appends one event with exact
revision digest; installs the complete state under CAS; and verifies any resolved command against
the exact expected event and checkpoint.

`load_review_replay` reads only the `AggregateKind::Review` chain, checks family/schema/sequence/
predecessor bounds, decodes the optional checkpoint, and returns a `ReviewReplay`. `rebuild`
semantically replays from genesis and requires checkpoint equality.

Migration version four performs SQLite table-copy widening for `aggregate_heads` and `events`,
changing only `CHECK (aggregate_kind BETWEEN 1 AND 8)` to `BETWEEN 1 AND 9`. It copies every column
in global/aggregate order, verifies source/destination row counts, recreates required indexes,
updates `store_meta`, `schema_migrations`, and `PRAGMA user_version`, and runs under the existing
backup/recovery engine. A checked-in exact version-three fixture containing representative tags
1–8 and historical protocol frames proves preservation and backup restoration.

### Alternatives considered

One C0 aggregate per reviewer cycle would isolate writes but make cross-review quorum, duplicate
reconciliation, oscillation, and terminal truth depend on a new multi-aggregate atomic protocol.
It would also make one current checkpoint incapable of proving finding conservation. A run-scoped
aggregate is preferred because D2 transitions are bounded and serialized by later E0, while the
complete current truth remains reconstructible from one chain.

Storing only B2 `ReviewObservation` values would reduce code but discard fixer responses,
reconciliation provenance, reviewer confirmation, stale history, and truthful escalation state.
The richer D2 domain model is required; B2 values remain projections.

Using untyped JSON submissions would be convenient for provider adapters but would make parser
behavior and defaults authoritative. D2 instead accepts checked Rust values and canonical B3
frames. Provider-specific JSON/SDK parsing remains outside D2 and must construct these values
through bounded constructors.

## Data and compatibility

Protocol family tags 53–55, aggregate tag 9, namespace `0xD201`, command/event variant tags,
finding/disposition tags, and canonical field order become immutable compatibility surfaces when
merged. Unknown tags are rejected, not interpreted as future defaults. New optional semantics
require a new schema version; changed authority or lifecycle meaning requires a new event/command
variant and architecture review.

Historical events are never rewritten. Revision advance and reconciliation append events and
retain prior records. Projection schemas may evolve, but the complete D2 checkpoint is a cache and
must always agree with genesis replay.

## Failure handling

- Constructor/reducer failure returns a stable `ReviewErrorKind` and actionable recovery class;
  it emits no event and no speculative state.
- Failure before SQLite commit leaves no event/checkpoint. Indeterminate commit is resolved by
  command identity and canonical request digest.
- A committed event with missing, corrupt, ahead, or behind checkpoint fails startup/rebuild; D2
  does not guess which state is current.
- Malformed or oversized submissions fail before quorum accounting. Later explicit failure events
  may describe orchestration failure without converting it into a review.
- Revision drift makes prior evidence stale deterministically. It does not delete evidence or
  silently carry a resolution/waiver forward.
- Cancellation waits only on D2's inert state; D2 owns no provider/tool/process effect that could
  remain detached.
- Oscillation and exhaustion produce explicit terminal evidence and preserve every finding and
  cycle needed for diagnosis.

## Security considerations

D2 treats reviewer/fixer/provider text and bytes as inert bounded evidence. It grants no
capability, executes no text, accepts no path for I/O, and has no ambient clock/randomness. Exact
revision, producer-independence, causal, and authority-observation checks fail closed. Human
waiver authority remains in B1/B2 and overall acceptance remains in B0/E0. This slice deliberately
does not expand into speculative adversary handling beyond realistic malformed, stale,
contradictory, duplicated, or corrupt records.

## Verification

Focused verification is serialized with `CARGO_BUILD_JOBS=1` because the development host has
full swap. The root runner executes, in order:

```text
cargo test --locked --package peritus-review --all-targets --all-features
cargo test --locked --package peritus-protocol --package peritus-journal --package peritus-migrations --package peritus-conformance --all-targets --all-features
cargo clippy --locked --package peritus-review --package peritus-protocol --package peritus-journal --package peritus-migrations --package peritus-conformance --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --package peritus-review --all-features --no-deps
cargo verus verify --locked --package peritus-review --all-features --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo run --locked --package xtask -- all
just gate-a
```

Workers may inspect and edit concurrently only under the frozen path ownership above. Before the
first build they are stopped; Cargo, rustdoc, Clippy, Verus, and Gate A are never concurrent. The
full Gate A runs once after focused checks and substantive QA are green, then once against fresh
merged main as repository policy requires.

## Rollout and rollback

D2 lands on a feature branch through a signed protected PR. All existing Ubuntu, macOS, and
Windows Gate A and Foundation jobs must pass before signed merge. Fresh-main hosted matrices and a
serialized local Gate A must pass before Crosslink closure.

Database migration requires a completed consistent backup. Rollback testing restores the pre-D2
version-three backup and opens it with the pre-D2 schema reader. Once D2 events exist, removing D2
support is not a compatible downgrade; rollback uses the backup or a later forward repair and
never rewrites historical events.

## Open questions

None. The user fixed tags, namespace responsibility, maximum-verification posture, resource
policy, authority boundaries, and delivery requirements in the goal.

## Out of scope

- E0 end-to-end writer/reviewer/fixer orchestration or overall acceptance.
- D3 scheduling, collaboration, worker pools, or distributed coordination.
- CLI, TUI, daemon, IPC, provider invocation, SDK/executable routing, prompt rendering, or model
  response parsing.
- Workspace/Git mutation, process/shell/tool execution, or sandbox control.
- Issuing capabilities, approvals, waivers, credentials, leases, or budgets.
- New enterprise/advanced CI runners or unrelated security hardening.
