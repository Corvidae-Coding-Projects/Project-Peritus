# GAP-03 scheduler production proofs

Status: candidate implementation, independent technical review, proof-impact inventory, and all
132 affected-package gates are locally qualified. Protected-base authorization and current-head
hosted qualification remain in progress. This work starts from develop commit
`a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8`, after GAP-02 authorization and application.

The complete scope is the GAP-03 row of [the stabilization checkpoint](formal-coverage-checkpoint.md),
with the behavioral requirements in [the D3 design](../.design/d3-scheduler-collaboration.md).
Local issue 77 tracks this work under the broader formal-verification campaign, issue 75.
Issue 78 tracks cancellation qualification; issue 79 retains worker-loss and whole-transition
composition. This delivery closes the remaining production-code gaps targeted on draft PR 77.
The [final candidate review](formal-scheduler-gap3/final-candidate-review.md) binds exact commit
`5d739de2902de0a5379f23f32b0c9f0ae562b28c` and its tree. Final proof-impact authorization and
hosted qualification are separate delivery gates.

## Required production relationships

| Requirement | Production surfaces | Completion evidence |
|---|---|---|
| Command admission | `reducer::{start,decide,validate_fences}`, `reducer::apply`, worker/work admission, identity lookup and sorted insertion | Exact acceptance and rejection conditions, error precedence and payloads, unchanged rejected input, and complete successful state/event relations. Sortedness and lookup misses must be established, not inferred from successful lookups. |
| Cancellation trees | `reducer::apply::control::cancel`, cancellation acknowledgement, production work updates | Exact root/descendant closure and canonical affected list; terminal descendants excluded from changes; inactive work cancelled; active ownership retained until acknowledgement; unrelated state preserved; no late-success resurrection. |
| Worker loss | `reducer::apply::lose_worker`, reservation release, worker update | Every affected reservation classified exactly once from its recorded recovery policy and remaining attempts; exact outcomes, release, work and worker changes; unrelated ownership and resource accounting preserved. |
| State transitions, replay and terminal outcomes | Every `reducer::apply` command variant, `start`/`decide`/`replay`, `command_from_event`, `DispatchNext` selection, dependency refresh, terminal evaluation, `pending_directives`, durability binding/rebuild/load and session/daemon callers | Exact selector success and failure, minimum/first-worker choice and OBL-0150; composition into exact accepted events and complete successors; rejected transitions leave input unchanged; replay agrees with live reduction and rejects invalid history; computed cancellation/loss/finalization payloads and pending directives are exact. Cursor, digest, append acceptance and effect boundaries remain explicitly accounted for. |
| Termination | Admission scans, cancellation closure and updates, worker-loss loop, refresh, replay, and terminal evaluation | Well-founded decreases for every relevant loop/recursive proof, including closure growth and nested traversals. Finite computation termination is distinguished from progress of external workers. |

Existing resource and lifecycle proof bodies, dependency-budget repairs, provider fixes, and
historical approvals must remain intact. Scheduler wire families 70, 71, and 72, stored formats,
public runtime interfaces, scheduling order, and diagnostics retain their current behavior unless
a demonstrated correctness defect requires a separately explained and tested change.

## Implementation order

1. Establish exact terminal-summary contracts used by finalization and decoded-state validation.
   Prove the existing classification precedence and the complete ordered non-success identity list
   for every input, including empty work and nonterminal records. Retain the canonical digest path.
2. Complete identity ordering, successful/missing lookup and insertion contracts, then the complete
   worker/work and command admission relationships that depend on them.
3. Prove cancellation closure and updates, and worker-loss processing over the strengthened state
   relations. Preserve the performance properties of the production operations.
4. Connect all command routes, dependency refresh, finalization, cursors and events to actual
   `start`/`decide`/`replay` behavior. Audit every relevant loop for termination.
5. Independently review the final specifications, implementation correspondence, source identities,
   tests and proof evidence; reconcile affected proof records through the existing trust checker.

This order expresses dependencies, not reduced acceptance criteria. Direct annotation of the
entire reducer at once would combine incomplete primitive contracts with codec/effect boundaries.
The preferred approach strengthens the existing production functions and composes their contracts.
Any extracted deterministic function must actually be called by production; an unconnected model
or stronger caller precondition does not close the corresponding requirement.

## Increment evidence

The first terminal-summary increment is implemented in `state/terminal/evaluation.rs`, called by
the existing `SchedulerTerminal::evaluate` wrapper. Its declarative specification establishes the
maximum recorded classification, the exact input-order list of all non-success identities without
deduplication, and completion exactly for empty/all-success input. The loop has an explicit
decreases measure. Finalization and decoded-state validation retain their existing wrapper and
canonical digest path; neither whole composition nor cryptographic execution is newly proved.

Retained local evidence under `target/gap3-evidence/`:

- `11-terminal-restored-verus.log`: strict Verus, 381 verified, zero errors.
- `09-classification-mutation.diff` and `.log`: changing cancelled work to completed fails the
  exact classification postcondition. `10-omission-mutation.diff` and `.log`: omitting cancelled
  identities fails the exact append precondition. Both runs exit 101 with one proof error; the
  original source was restored byte-for-byte after each mutation.
- `07-scheduler-tests-final.log` and `08-scheduler-clippy.log`: 31 scheduler tests passed and strict
  Clippy passed for that increment. Tests include all 49 ordered terminal-outcome pairs, public
  finalization and rejection, replay, decoded-state validation, and two fixed v1 digest vectors.
- The Sol 5.6 xhigh agent `/root/gap3_identity_order` independently reviewed the four terminal
  source/test files and passed this bounded increment. Its [review record](formal-scheduler-gap3/terminal-review.md)
  retains the source hashes and evidence references. This is agent review, not a human owner
  authorization or obligation discharge.

The next increment strengthens the existing identity byte comparisons and binary searches without
changing their executable algorithms. Found results denote the complete retained record. A miss
establishes absence under the declared canonical ordering predicate. Worker/work insertion now
proves its exact lower-bound position and preserves strict ordering for fresh identities. Genesis
establishes empty ordered collections. `13-insertion-verus.log` reports 400 verified and zero errors,
and `14-scheduler-tests.log` passes 35 tests, including admission/dispatch/acknowledgement/replay
lookup checks against independent linear search at every identity byte and search boundary.

The [independent lookup/insertion review](formal-scheduler-gap3/lookup-review.md) binds that earlier
snapshot by source hashes. Negative probes `17-lookup-miss-mutation` and
`18-insertion-slot-mutation` reject, respectively, an invented missing worker and the wrong
end-insertion position. `19-insertion-restored-verus.log` passes 400 verified, zero errors after
both byte-exact restorations. `16-scheduler-clippy.log` passes strict Clippy.

Subsequent local work extends canonical insertion to live reservations and historical dispatches,
proves reservation removal preserves ordering, and carries all collection ordering through the
actual state clone. It also contracts the existing state/binding accessors needed for command
fences. `27-clone-fields-verus.log` passes 415 verified, zero errors.

This composition exposed a specification defect in both pre-existing state clone predicates:
unparenthesized quantifiers captured the later conjunctions. Empty workers could make work,
reservations, histories, ordinals and terminal correspondence vacuous; later empty collections
could likewise hide their trailing constraints. All six quantifiers are now explicitly grouped.
The runtime clone is unchanged. Its new proof projection requires every
authoritative collection length, cursor, history and terminal relation without a nonempty-state
precondition. Restoring the old grouping in `28-clone-grouping-mutation.diff` makes its work and
reservation length postconditions fail (`28-clone-grouping-mutation.log`: 414 verified, one failing
verification item with two reported postcondition errors). The original source was restored
byte-for-byte. The [independent ordering/clone review](formal-scheduler-gap3/ordering-clone-review.md)
confirmed the original vacuity and the grouping repair. `30-restored-scheduler-verus.log` passes
415 verification items, `33-scheduler-tests.log` passes 35 tests, and `34-scheduler-clippy.log`
passes strict Clippy for that source snapshot.

Worker-descriptor admission now calls a verified classifier with exact canonical-class,
concurrency and capacity predicates, successful field correspondence, and first-rejection
priority. The ordinary public wrapper retains the existing error kinds, recovery, and diagnostic
strings. Execution-class rank is independently specified and tested against derived Rust ordering.
The existing resource-entry accessor projects its representation invariant; no new caller
precondition is introduced. A terminating queue-count loop also proves the exact number of
Queued/WaitingDependencies/RetryPending records used by current work admission.
`39-admission-verus.log` passes 422 verification items with zero errors.
`40-worker-descriptor-tests.log` found a test-fixture error conversion; the fixed nonzero actor
fixture now uses a checked expectation, and `41-worker-descriptor-tests.log` passes all four
descriptor tests. Neither the ordinary error
wrapper nor whole work admission is discharged by these results.

The existing `validate_fences` wrapper now calls `reducer/fences.rs::classify`, whose exact result
specification preserves terminal rejection before the command-history ceiling, then rejects any
run, seven-component revision, sequence, predecessor, digest, reused identity or genesis-payload
mismatch. Byte comparison contracts state direct array equality. Every comparison/history loop
has a finite decreases measure. The public wrapper retains all existing diagnostic and recovery
mapping. `SchedulerCommand` exposes exact field projections and has an explicit verified clone
covering all eight fields, including the existing semantic payload clone relation.

Worker loss now calls a verified ordered filter/projection of the pre-transition reservations
and an exact classifier for each retained work record. Its five cases preserve cancellation
dominance, the strict remaining-attempt condition for retry, and the original dispatch/work
identities. The ordinary release loop retains the same terminal payloads, digest inputs, errors
and outcome order. `LossOutcome` opts directly into verification with `verifier::verify`; this
keeps its declaration and exact clone checked without generating unused enum projection methods
that lose documentation in the pinned macro. No lint allowance or trusted body is introduced.

`54-restored-fences-loss-verus.log` completes successfully with 448 verified, zero errors after
four guarded negative probes. `50-descriptor-max-mutation` rejects refusing valid maximum
concurrency; `51-fence-history-mutation` rejects admitting a full command history;
`52-loss-owner-mutation` rejects selecting another worker's reservations; and
`53-loss-attempt-mutation` rejects requeuing an exhausted attempt. Each run reports 447 verified
and one proof error, exits 101, and restores its source byte-for-byte. Raw diffs and logs are
retained with the reproducible native-shell driver under `target/gap3-evidence/`.

The formatted source snapshot is recorded in `60-source-snapshot.sha256`. Its final ordinary
run, `58-scheduler-tests-final.log`, passes 45 tests and fails the two retained queue/recovery
regressions, with no ignored tests. This includes all seven revision components, every identity
and digest byte, exact error mapping and unchanged rejected input, plus twelve mixed-worker
loss scenarios covering all cancellation policies, attempt boundaries and acknowledgement
states. `57-scheduler-clippy.log` passes strict all-targets/all-features Clippy, and
`59-format-check.log` records a successful formatting check. At that snapshot the combined test
command failed; these historical records do not establish a green delivery checkpoint. The later
versioned repair below supersedes its queue failures while retaining the reproduction evidence.

These contracts do not yet compose the entire worker-loss mutation loop, resource deltas,
successor event, error wrapper, or whole reducer/replay relation. Those remain acceptance
requirements, not exclusions.

### Reproduced queue/recovery inconsistency

`35-queue-recovery-reproduction.log` records two failing public-command regressions in
`tests/queue_recovery.rs`. With queued_work=1, dispatching retryable A frees the visible queue
slot and admitting B fills it. Either Retryable failure of A or RetrySafe worker loss then
returns A to the queue. Both command sequences are accepted and replay exactly, yet their
encoded checkpoints fail domain validation on decode because two waiting records exceed the
immutable queue bound. This is a runtime defect, not a missing annotation.

Admission pressure that includes recoverable Reserved/Running work would prevent the defect:
count existing queued/waiting/retry-pending records plus Reserved/Running records with an
attempt remaining. Explicit Retryable failure is permitted for every recovery policy, so the
second category must not be restricted to RetrySafe. Cancellation, final attempts and terminal
outcomes release this pressure. Recovery itself must remain able to release reservations.

That correction changes previously accepted admission decisions. Schema-v1 replay runs the
same reducer, so it would also reject benign old histories where A eventually succeeded.
The original binding had no semantics-version field. The
[independent queue review](formal-scheduler-gap3/queue-recovery-review.md) confirms the transition
analysis and compatibility issue. Existing histories are preserved by the explicit versioned
repair described below. The original reproduction output is retained.

### Versioned queue/recovery repair

The [implemented design](formal-scheduler-gap3/queue-version-design.md) gives each aggregate
immutable semantics, encoded only by its B3 schema header. New public bindings and commands use
schema 2. Old schema-1 commands, events, checkpoints, canonical preimages and journals retain
their historical decisions and exact bytes; their bounded recovery states now decode correctly.
No database rewrite, namespace change, or deployment is involved.

Production admission and checkpoint validation call `state/queue.rs`. Its terminating count
loop and exact specifications establish waiting/recoverable-active pressure for schema 2 and
waiting/active occupancy for schema 1. The old `reducer/apply/admission.rs` count helper is
replaced by this shared accounting. Semantic equality is included in command fences, replay,
state clone relations and durable binding checks. Actual event kinds and event clones now have
checked complete clone relations, including ordered loss and cancellation payload vectors.

Dual codec helpers select versions from checked headers or immutable value semantics. The
transport registry distinguishes current and supported versions, and app bindings, subscriptions,
projection and evidence use supported versions. Durable creation accepts only schema-2 genesis
after resolving exact existing commands. Existing aggregates reject mixed versions. The daemon
uses the same dual decoder and reports unsupported fresh legacy genesis before append; E0
lifecycle commands derive semantics and fences from their actual replayed predecessor.

`62-base-history-export.log` records successful capture from unchanged protected base
`a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8`. The 45 immutable command/event/checkpoint frames in
`fixtures/protocol/scheduler-v1-recovery` cover benign completion, retryable failure and worker
loss. Each history includes an admission that strict schema 2 correctly rejects. The existing
`scheduler-v1` fixtures remain unchanged; `scheduler-v2` adds separate current-schema vectors.

Current local qualification evidence:

- `84-restored-versioned-queue-verus.log`: strict scheduler Verus passes, 466 verified and zero
  errors after four deliberately broken implementations each fail their corresponding proof.
  `80` through `83` retain the mutations, output, and byte-exact restoration evidence.
- `85-final-versioned-queue-tests.log`: all 62 scheduler tests pass, with no failures or ignored tests.
  This includes recovery under all policies and acknowledgement states, repeated retries through
  the final attempt, legacy byte-preserving replay and continuation, real C0 restart/import,
  exact existing-genesis retry, cross-schema conflicts, zero-write mixed-schema rejection,
  hostile decoded queue bounds, mixed-version event chains, and checkpoint/event version mismatch.
- `68-schema-consumer-tests.log`: 144 tests pass across protocol, app protocol, projection and
  evidence. `91-final-versioned-queue-clippy.log`: strict Clippy passes for those four packages plus
  scheduler, daemon and performance qualification, with all targets and features.
- `71-v2-fixture-generation.log`: the new schema-2 corpus is generated and checked while the
  original schema-1 corpus is only compared. All three fixture digest manifests verify.
- `89-daemon-library-tests.log`: 178 tests pass, zero fail, and three existing tests remain ignored
  (one subprocess child fixture and two graphical environment fixtures). The executed tests cover
  actual fresh-version admission, settled legacy retries, both pending and indeterminate legacy
  reconciliation, and same-ID version relabel conflicts without an extra journal append.
- `87-protocol-codegen-check.log`, `88-workspace-check.log`, and `90-format-check.log` pass the
  generated protocol check, workspace all-targets/all-features compilation, and formatting check.
- `92-versioned-queue-source.sha256` binds the 980 source, test, fixture, generated protocol,
  architecture and design files examined for this increment. Its digest check passes.

The [independent queue-version review](formal-scheduler-gap3/queue-version-review.md) passes this
bounded increment against snapshot 92 and the final evidence above. The corresponding source
archive is retained as `92-versioned-queue-source.tar`; later cancellation edits are outside that
review. These queue classifier and clone contracts do not yet prove that every complete reducer
transition preserves the selected queue invariant.

These are local dependency increments. Complete admission, all ordering producers and decoded
validation, cancellation, worker loss, selector/reducer/replay composition, remaining loop
termination, whole-path caller qualification, final GAP-03 review and source/trust reconciliation
remain open.

### Cancellation closure and update composition

Production `control::cancel` now calls `cancellation::cancel_retained`. Its selector defines
reachability independently as membership in every parent-closed set containing the requested
root. Parent closure includes terminal records; only the final emitted work subsequence excludes
them. Canonical input yields a canonical, duplicate-free affected sequence, and every emitted
identity is retained in the pre-state. No caller precondition or acyclicity assumption was added.

The existing whole-scan fixed-point algorithm now uses verified binary membership and ordered
insertion instead of repeatedly sorting the entire growing vector. Each successful insertion
strictly decreases the finite retained-identity universe minus the reached set. Missing parents,
connected cycles and disconnected cycles therefore do not undermine termination. The inherited
worst-case whole-scan and per-target update costs remain; this increment makes no linear-time
performance claim.

Actual work setters now preserve the complete immutable `WorkSpec`, enqueue ordinal, bypasses,
and attempt count. Their exact effects distinguish phase-only changes, retry causes, and the
supplied terminal payload. Every non-target record and every other scheduler field is preserved.
The new update loop composes those effects against the original reservation sequence: active
targets become Cancelling while retaining ownership, and inactive targets receive Cancelled.
Its contract also accounts for the exact successful prefix before a missing target. Selection
establishes that this missing-target branch cannot occur in `cancel_retained`.

`111-final-cancellation-verus.log` passes 498 verification items, including the connected update
loop and exact reservation-removal return contract, with zero errors. The first composition gate
(`104`) exposed a proof resource limit and an incomplete success-witness proof. Checked helper
composition and an equivalent direct success predicate resolved both with unchanged verifier
limits. The final file-layout split separates record-preservation lemmas, trace composition and
the actual loop; it adds no theorem or trust exception.

`113-final-checkpoint-tests.log` passes 70 scheduler tests, including public cancellation,
reservation retention, late-success rejection, replay/checkpoint roundtrips, and five direct
arbitrary-parent-graph tests. `112-final-checkpoint-clippy.log` passes strict all-targets,
all-features Clippy for the seven affected packages. The final post-layout qualification is
recorded in the checkpoint evidence below.

`remove_reservation` now also proves that a returned reservation is exactly the old sequence
element removed. A missing reservation leaves the sequence unchanged and proves absence; every
non-reservation state field is unchanged in both cases. This does not yet compose terminal
release or acknowledgement outcomes.

Beyond that local qualification, cancellation still needs the complete root-admission/error/event
relation, readiness and selected-queue-invariant composition, exact terminal release,
acknowledgement/completion outcome and no-op contracts, and the stateful proof that late
completion cannot resurrect work before or after acknowledgement.
Existing invariant-preservation contracts and the standalone Boolean cancellation theorem do not
replace those exact relationships. Whole reducer, replay and caller composition remain required.

### Checkpoint integration fixes

Repository qualification found seven oversized files. Private modules now separate schema family
data, event kinds and loss payloads, cancellation trace/frame lemmas, indexed work mutation and
test fixtures. Existing public reexports, specification bodies and test assertions are retained;
no source-budget exception was added.

Formal-boundary unit tests now read the same immutable fixture paths with `std::fs::read` and the
audited `CARGO_MANIFEST_DIR` value. This avoids unsupported embedded-data macros without altering
the historical fixtures. The API checker recognizes only the exact
`cfg_attr(verus_keep_ghost, verifier::verify)` form used by the pinned Verus source. Hostile
conditions, extra attributes and trusted-body variants remain rejected, and a regression checks
that the marker cannot hide an executable precondition. The fence classifier's private name is
now `classify`, avoiding confusion with Verus's trusted `admit` construct; its body and contract
are unchanged.

The final source manifest contains 1,196 entries across the affected packages, checker,
architecture, design, generated artifacts and fixtures. It is retained with selected raw logs in
[the checkpoint evidence](formal-scheduler-gap3/evidence/README.md). Historical snapshot 92 and
its original review remain intact; its compressed source archive preserves the pre-cancellation
and pre-layout source needed to reproduce those hashes.

The [final independent cancellation review](formal-scheduler-gap3/cancellation-review.md)
passes this bounded increment against snapshot 140. Final local qualification passes 498
scheduler verification items, 109 scheduler/protocol/projection tests, 178 daemon tests and 45
API-checker tests; strict Clippy, formatting, generated protocol, architecture and ordinary-API
checks also pass. Three pre-existing daemon fixtures remain ignored. Protected trust is not yet
qualified: its recorded failures are exactly 49 stale and 38 missing proof-impact fingerprints.
This local candidate is frozen before the separate authorization/application sequence; none of
these local successes is represented as final hosted qualification or an obligation discharge.

### Cancellation command composition on draft PR 77

The production root-cancellation kernel now relates each rejection to the unchanged input state
and each success to the exact `WorkCancelled` event and affected work sequence. The actual
cancellation loop preserves reservation readiness and collection ordering, allowing subsequent
reservation commands to use their checked lookups. The ordinary error wrapper retains its
existing diagnostic kinds and text.

Terminal release now relates the exact removed reservation, supplied terminal payload, updated
work record and every unaffected scheduler field. Its ordering guarantee composes with the
production completion and cancellation-acknowledgement kernels. Those kernels classify each
outcome and prove rejected valid-state commands leave the complete state unchanged. Their exact
outcome contracts require readiness and collection ordering in the pre-state; this condition is
explicit rather than imposed as a new executable precondition.

The existing public regression exercises Reserved, Running and Cancelling descendants, ownership
retention, acknowledgement, late completion on both sides of acknowledgement, and replay plus
checkpoint roundtrips. The command-specific modules keep the source budget without changing the
existing public reexports. This increment does not claim whole-reducer or whole-replay proofs.

Final validation passes 505 strict scheduler verification items, all 70 scheduler tests, strict
Clippy, formatting, architecture and ordinary-API checks. The
[independent command-composition review](formal-scheduler-gap3/cancellation-command-review.md)
passes this bounded increment against the 142-file scheduler snapshot 157. Its raw output and
source manifest are retained in [the evidence directory](formal-scheduler-gap3/evidence/README.md).

Checkpoint `5020fddf692b0894c6c8d188efc3490a3419b8c8` and subsequent work share
[draft PR 77](https://github.com/Corvidae-Coding-Projects/Project-Peritus/pull/77), targeting
`develop`. The PR remains a draft and must not be merged as part of this work. Remaining work goes
on `feature/formal-scheduler-gap3` and that same PR.

### Work admission and cancellation capacity increment

The production `AdmitWork` path now calls an exact verified kernel. Its contract preserves all
nine rejection priorities, unchanged rejected state, the admitted definition and event, initial
work fields, canonical insertion, ordinal advancement, retention limits and versioned queue
pressure. The dependency and worker scans terminate. Public tests cover competing rejection
conditions and preserve the existing rule that Lost and Draining workers can witness admission
capability until Removed; accepted histories still replay and decode exactly.

Cancellation now preserves the queue bound through each actual update, the complete selected
batch and successful root command. A proof over the production completion/acknowledgement
relations establishes that late completion is an exact no-op rejection, acknowledgement removes
ownership and records Cancelled, and later completion remains an exact no-op rejection. The
remaining link from the root command relation to this chain's initial cancelling-dispatch premise
is explicit below; this is not a claim of whole public-command composition.

Worker loss now calls an individually verified classification/release kernel. It binds the
pre-release work policy to the exact outcome, removed reservation, lifecycle result and other-state
frame, preserving reducer readiness and ordering. Digest values still come from the existing
ordinary SHA-256 boundary. The full loop, worker/event composition and loss-specific queue-bound
proof remain open. In-progress batch work is excluded from this checkpoint.

The [bounded independent review](formal-scheduler-gap3/admission-cancellation-review.md) and
source manifest 182 identify this increment relative to `39f56c636`. Historical reviews and
qualification remain attached to their original source snapshots.

### Worker-loss batch and replay reconstruction increment

The actual worker-loss path now executes a verified release batch. Its contract ties the selected
dispatches to a finite, exact sequence of individual releases, the ordered `WorkerLost` outcomes,
and the final Lost worker phase. It preserves unrelated records, worker descriptors, reservation
readiness, collection ordering and both versioned queue bounds. The existing SHA-256 wrapper
supplies each dispatch's failure digest; the proof establishes exact propagation of those values.
The public regression covers empty loss, missing/lost/removed worker errors, mixed recovery
policies, exact ownership/resource effects and replay. Exact rejection contracts and the outer
reducer/caller composition remain open.

The cancellation lifecycle theorem now derives its initial cancelling-dispatch premise from the
successful root command's exact production relation. It composes the actual cancellation,
completion and acknowledgement contracts to exclude late-success resurrection, retaining queue
capacity throughout. The theorem does not yet appear in a whole-reducer/replay invariant.

Production replay now calls the contracted event-to-command reconstruction. All event variants
map to their exact causative command inputs, including the cancellation-tree flag and dispatch
token. Derived loss/cancellation/finalization outputs remain subject to replay's existing event
comparison. Event fields, transition construction/clone/access and cursor/digest mutations have
exact contracts. Pause, resume and drain produce exact typed events or unchanged-state rejections.
These relationships do not claim a proof of the outer replay loop, cryptographic execution or
every command successor.

The combined source passes 590 strict verification items and all 77 scheduler tests. A guarded
negative probe reverses the replay cancellation-tree flag and fails the reconstruction
postcondition; its source is restored byte-for-byte. Separate public replay tests reject a
tampered successor digest and a tampered derived cancellation list. Retained source identities,
gate output and independent bounded reviews accompany this increment. Dependency and worker
refresh drafts remain separate pending integration and validation.

Source manifest `211-composition-source.sha256` binds 168 scheduler paths relative to checkpoint
`c6db6c9e6`; its SHA-256 is
`c7a5b16af1112d16e0cb859cd54ef325b85369976acde7b71944ae86cbd2ffc4`.
The [cancellation/reconstruction review](formal-scheduler-gap3/cancellation-reconstruction-review.md),
[worker-loss review](formal-scheduler-gap3/worker-loss-batch-review.md) and
[integration review](formal-scheduler-gap3/reducer-integration-review.md) identify their distinct
reviewers and author exclusions. These bounded technical reviews are not human approvals or
final protected authorization.

### Dependency scanning, worker refresh and finalization increment

Dependency propagation now calls a verified scan and ordered change collector. The scan handles
missing and nonterminal dependencies, chooses the first failed dependency in specification order,
and makes waiting work ready only when every dependency succeeded. The collector emits exactly
the actionable records in retained work order. Their loops terminate. The outer dependency update
and fixed-point loop still require composition and a termination proof; this increment does not
claim that a passing scan proves that whole loop.

On ready, ordered scheduler states, the actual worker refresh loop now proves every final phase
from the original reservation snapshot. Draining, Lost and Removed workers remain unchanged;
other workers are Busy exactly
when owned reservations reach concurrency, including ownership retained during cancellation.
Descriptors, unrelated scheduler fields, readiness, ordering and queue bounds are preserved.
The reused reservation counter now uses a reverse-index loop with its existing exact contract,
preserving constant stack use instead of introducing recursive counting into refresh.

Finalization retains its exact existing admission and error order. A read-only prepare pass
checks terminal work and empty reservations, then summarizes work once. The ordinary adapter
hashes that summary once; the verified commit installs the supplied digest and exact summary in
both state and event, changing only scheduler phase and terminal fields. The prepared-summary
clause is conditional on the plan matching the input work; the immediate production
prepare/hash/commit path supplies that premise without an intervening state mutation. Cryptographic
execution and the outer reducer remain explicit boundaries.

Public regressions exercise reverse-identity dependency cascades, deterministic first failure,
the Available/Busy concurrency threshold, cancellation retaining ownership until acknowledgement,
unchanged inactive workers and exact replay. Inverting the counter's worker predicate fails its
exact loop invariant; the original source is restored byte-for-byte before final qualification.
The [refresh review](formal-scheduler-gap3/refresh-review.md) and
[finalization review](formal-scheduler-gap3/finalization-review.md) identify independent reviewers
and their author exclusions. Retained final evidence and source identities are listed in
[the evidence directory](formal-scheduler-gap3/evidence/README.md).

Final qualification passes 628 strict verification items and all 79 scheduler tests, strict
Clippy, formatting, architecture and ordinary-API checks. Manifest `228c-refresh-source.sha256`
binds 176 scheduler paths relative to `8644a2d2b`; its SHA-256 is
`f3682e9867d5e449f36c6db291a58244716d9df99af38c926adb3531af596aab`.

### Dependency, selection and reducer completion increment

Production dependency refresh now reaches a verified fixed point with a finite two-step budget per
retained work item. Each immutable round has an exact action trace, preserves identity layout and
unrelated state, and strictly reduces the natural WaitingDependencies/Queued measure on valid
states. Production selection now proves the exact first feasible worker and exact aged, priority,
enqueue-ordinal and work-identity ordering. The implementation retains the original concurrency
short-circuit before resource scans.

`DispatchNext` now passes through a production-called typed adapter that preserves the existing
rejection order, error kinds and diagnostic strings and relates successful events to the exact
selected work, worker, dispatch identity and token. Pending directives have an exact canonical
reservation-order filter/map and configured batch cutoff for unacknowledged dispatch and
cancellation effects. Production `start` and `decide` call verified preparation and commitment
seams for genesis, cursor, digest and event construction. State-size checks, SHA-256 execution and
semantic command application remain explicit ordinary boundaries.

The final combined source passes 693 strict verification items and all 79 scheduler tests. Strict
Clippy, formatting, architecture and ordinary-API checks pass. The tests include deterministic
replay equality, tampered-history rejection, restart/session callers, dispatch rejection priority,
dependency cascades, selection fairness and both scheduler wire versions. Manifest
`257-final-scheduler-source.sha256` binds all 189 scheduler package files; its SHA-256 is
`fe6b85f563ed3cbc6ca990b3e6e6d041734eb2592da0ba4a23416b911bd67240`.

## Remaining requirements on the draft

| Requirement | What this checkpoint establishes | What remains open |
|---|---|---|
| Admission | Exact work admission, dispatch selection/rejection, command fences, canonical retention and queue pressure; unchanged error mapping | State-size and hashing execution remain ordinary checked boundaries |
| Cancellation | Exact root/event and descendant updates, queue/readiness preservation and non-resurrection; public lifecycle and replay regressions | Cryptographic digest execution remains an ordinary boundary |
| Worker loss | Complete finite release trace, exact recovery outcomes, worker/event changes and ownership/resource/queue preservation | Cryptographic loss-cause digest execution remains an ordinary boundary |
| Transitions and replay | Exact terminal, phase, dependency fixed point, selection, pending directives, genesis/cursor/digest/event commitment and reconstruction contracts; deterministic replay and caller regressions | Replay's standard-library set membership and event equality are validated by executable tests rather than a new Verus model |
| Termination | Checked decreases for classifiers, cancellation, worker loss, dependency fixed point, selection, refresh, finalization and directive loops | External worker progress is outside the pure scheduler |
| Delivery | Frozen source manifest and complete local scheduler qualification | Independent final review, reconciled fingerprints/authorization/trust and current-head hosted workflows |

The existing CI/checker implementation from GAP-02 remains in place. No branch protection,
maintainer self-merge permission, historical approval or obligation status changes in this
checkpoint.

## Verification and evidence

Use pinned Rust 1.97.1 and Verus 0.2026.08.09.92f466f. Keep one Cargo build job and at most two
Verus CPUs. Retain raw output under `target/gap3-evidence/` while work is in progress.

Required checks include strict package verification with `--no-cheating`, the scheduler ordinary
tests, formatting and strict Clippy, unchanged wire/replay fixtures, actual daemon/session caller
tests, and the affected source/API/inventory/trust gates. Independent expected results and negative
proof mutations must demonstrate that wrong classification, omitted descendants, incorrect loss
outcomes, stale fences, and replay divergence are rejected by the relevant contracts.

Verification of a helper, a cached prior result, or ordinary tests alone cannot establish the
complete production relationship. Unsupported boundaries require demonstrated limitations and
explicit reviewed contracts and compensating evidence; difficulty alone is not an exclusion.
