# D2 review engine

`peritus-review` is the durable, deterministic boundary for independent review cycles and the
complete lifecycle of their findings. It consumes checked B2 acceptance contracts, exact candidate
revisions, C6 reviewer-context facts, inert reviewer and fixer records, and externally authorized
waiver observations. It does not call a provider, execute a tool, mutate a workspace, issue a
waiver, or accept a run.

## Trust and authority

Every review run is bound to one checked `AcceptanceContract`, its exact `ContractBinding`, the
complete review-policy snapshot, an exact seven-component `RevisionTuple`, candidate digest, tree
digest, and the identities and ancestries of the candidate's producers. `ReviewBinding` computes a
domain-separated digest over that complete input. A caller cannot carry review evidence to another
contract, candidate, tree, or revision by changing only an outer identifier.

D2 accepts three kinds of inert input:

- reviewer assignments and completely validated structured submissions;
- fixer responses and reviewer confirmations; and
- B1/B2 authority observations, including an already-authorized `WaiverObservation`.

None of those inputs gains authority merely by appearing in a model response or a D2 event. D2
checks and records the observation. B0/E0 remains responsible for overall run acceptance, B1 owns
approval authority, and B2 owns the acceptance contract and quality evaluation.

## Lifecycle

A caller drives the pure state machine in this order:

1. Construct `ReviewLimits` and a checked `ReviewBinding` from the immutable B2 contract, exact
   revision, candidate/tree digests, and producer provenance.
2. Create the fenced genesis command and call `start`. Commit the resulting event and complete
   checkpoint before presenting the run as durable.
3. Assign a reviewer with a unique cycle identity and ordinal, nonempty canonical category set,
   checked `ReviewerIdentity`, exact C6 context-plan digest, fresh-context fact, and independence
   view.
4. Admit at most one completely validated `ReviewSubmission` for that assignment. Invalid or
   partial data is rejected before it can count toward quorum.
5. Reconcile genuine duplicates, record fixer responses, and record reviewer confirmations or an
   exact externally authorized waiver observation as findings progress.
6. Advance the binding when any revision, candidate, or tree component changes. Historical cycles,
   findings, responses, confirmations, and waivers remain auditable but cease to count as current.
7. Finalize only after current quorum is complete and every current finding has one permitted
   disposition. Explicit cancellation, failure, budget exhaustion, or deterministic oscillation
   ends with its truthful non-success result.

Every post-genesis command fences the expected run, revision, aggregate sequence, predecessor
event, prior-state digest, successor event identity, and idempotency command identity. If any fence
is stale, reload and replay. Do not edit a stale command to resemble the new head.

## Reviewer assignments and submissions

An assignment declares exactly which contract categories the reviewer covers. Its
`ReviewerIdentity` carries the reviewer actor, context, provider, model family, independence flags,
and ancestry used by B2. The identity's context digest must equal the assigned C6 context-plan
digest; a fresh-context boolean cannot substitute for that structural binding.

A submission is admitted atomically. Its categories must be a nonempty canonical subset of the
assignment and contract declarations. Its findings must be bounded, uniquely identified, and
internally consistent. Unknown categories, duplicate identifiers, invalid locations, noncanonical
sets, oversized text, or contradictory revision/source data reject the whole submission. An
assignment that has submitted cannot submit again.

Cancelled or invalidated cycles, assignments without submissions, malformed attempts, and records
from an old binding are retained but never count toward current quorum.

## Findings and dispositions

Each finding has a stable identity and retains its original cycle/reviewer, all reconciled source
cycles and reviewers, category, severity, derived blocking status, confidence, requirements,
locations, evidence references, description, reproduction, expected behavior, remediation, exact
revision, normalized digest, and append-only disposition history.

A finding begins open. A fixer response records evidence but does not close it:

| Fixer response | State before reviewer or authority action |
| --- | --- |
| `Fixed` | Open pending a current independent reviewer confirmation |
| `Disputed` | Open pending reviewer-confirmed invalidation or another disposition |
| `SupersessionProposed` | Open pending provenance-preserving reviewer confirmation |
| `WaiverRequested` | Open pending an exact external B1/B2 waiver observation |

A current finding is conserved until one of four dispositions exists for the same current
revision: reviewer-confirmed resolution, reviewer-confirmed invalidation, confirmed supersession,
or an externally authorized waiver. There is no implicit closure path.

Duplicate reconciliation selects an existing canonical finding. It preserves every absorbed
finding identity, reviewer/cycle source, evidence reference, and disposition record. The absorbed
record remains addressable as historical and points to the canonical finding. Cycles,
self-supersession, category/revision mismatch, conflicting prior supersession, or lost provenance
are rejected.

## Independent quorum

`QuorumReport` exposes each required B2 dimension separately:

- current submitted-review count;
- required-category coverage;
- distinct reviewer identities;
- producer independence;
- distinct C6 context digests;
- distinct model families;
- distinct providers;
- no shared reviewer/producer ancestry; and
- valid fresh reviewer context.

The report does not collapse these checks into reviewer count. A report can therefore explain the
single unmet dimension without implying that the rest passed or failed. Only completely validated,
current-revision submissions participate.

## Revision changes and oscillation

`AdvanceRevision` accepts a complete newly checked binding. A difference in any
`RevisionTuple` component, candidate digest, or tree digest makes prior evidence stale. The engine
keeps the old data for diagnostics and replay but excludes it from current projections, quorum,
finding conservation, waiver use, and finalization.

D2 detects bounded non-progress from canonical current finding fingerprints and severity history.
Repeated finding sets, configured stagnation or regression, incompatible reviewer conclusions,
the contract review-cycle ceiling, and explicit budget exhaustion are durable terminal inputs.
Expected review disagreement or non-progress becomes `NeedsHuman`; an explicitly unrecoverable or
integrity failure becomes `Failed`. Neither condition is translated into completion.

## Terminal truth

The closed terminal set is `Completed`, `NeedsHuman`, `Failed`, and `Cancelled`.

`Completed` means only that the D2 review boundary is complete: every enabled current quorum
dimension passes and every current finding is conserved by a permitted current disposition. It is
an observation consumed by later orchestration, not acceptance authority. Missing reviews,
findings, evidence, confirmations, or authority observations cannot become success through a
default, timeout, malformed submission, cancellation, or exhausted limit.

## B2 projections

`QualityProjection` emits only B2-owned values:

- `ReviewObservation` for each current validated submission;
- `FindingObservation` for each current canonical finding and its current disposition; and
- `WaiverObservation` values that D2 previously consumed from the external authority boundary.

Projections are deterministic, read-only, non-authoritative, and rebuildable. An open, fixed, or
disputed finding projects as open; a waiver request projects as requested; a reviewer-confirmed
resolution projects as resolved. Historical and duplicate-superseded records remain in D2 state
but do not masquerade as current observations.

## Durability and restart

D2 owns canonical schema-version-one frames:

| B3 family | Payload |
| --- | --- |
| 53 | Fenced review command |
| 54 | Immutable review event |
| 55 | Complete review-state checkpoint |

The aggregate kind is `Review` with permanent C0 tag 9. Checkpoints use namespace `0xD201` and a
domain-separated run key. `commit_review_transition` atomically appends the family-54 event and
installs the family-55 successor state under aggregate-head and state-revision compare-and-swap.

The canonical family-53 command digest is the idempotency key. Repeating the exact command after a
lost acknowledgement returns the already committed event/checkpoint only when every byte and
semantic fence matches. Reusing the command identity with other bytes is a conflict. If the
aggregate advanced, reload the aggregate rather than treating the old local state as durable.

On restart:

1. Call `load_review_replay` for the run aggregate.
2. Rebuild from genesis with the immutable binding inputs.
3. Require every record identity, family, schema, sequence, predecessor, revision, event, state
   digest, and reducer transition to validate.
4. Require the installed complete checkpoint to equal semantic replay field for field and in its
   canonical encoded bytes.

Missing, ahead, behind, corrupt, foreign, or divergent checkpoints fail closed. Preserve the store
and immutable inputs for diagnosis; do not synthesize a checkpoint from whichever copy looks newer.

## Schema-version-four migration

C0 schema version four widens only the closed aggregate-kind checks from tags 1–8 to 1–9. The
migration requires a completed backup, copies constrained journal tables, verifies row counts and
metadata, recreates indexes, and then records schema/user version four. The checked version-three
fixture proves that all historical tag 1–8 rows and event bytes survive migration and that the
backup restores the exact pre-D2 store.

Once tag-9 events exist, an old binary cannot open that forward schema. Rollback restores the
version-three backup or uses a later reviewed forward repair; it never rewrites historical events.

## Errors and recovery

`ReviewError` exposes a stable kind, bounded diagnostic detail, and `ReviewRecoveryAction`.
Recovery distinguishes correcting caller input, replaying the aggregate, gathering a fresh review,
reconciling a finding, obtaining external authority, escalating to a human, and quarantining
integrity failure. Diagnostics intentionally carry no raw provider response, source contents,
secret, environment value, or arbitrary unbounded text.

## Operational checks

The ordinary Rust predicates `evidence_is_fresh`, `findings_are_conserved`,
`no_implicit_success`, `quorum_is_complete`, `replay_equivalent`, and `transition_is_legal` expose
the same core invariants used by the reducer. Matching Verus proof roots cover bounded arithmetic,
freshness, disposition legality, independent quorum, finding conservation, terminal truth,
oscillation bounds, and replay equivalence.

Before integration, run the focused D2 tests, strict Clippy, rustdoc, no-cheating Verus verify and
build, protocol/migration/conformance tests, and then the complete Gate A. On resource-constrained
hosts keep `CARGO_BUILD_JOBS=1` and never overlap Cargo, rustdoc, or Verus processes.
