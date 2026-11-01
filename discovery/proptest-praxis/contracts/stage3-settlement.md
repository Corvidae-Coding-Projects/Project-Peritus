# Stage 3 product review and candidate settlement

Source baseline: `d106c9b13318fbcdd3d816ed63ae48532806fa7e`, plus the coordinating discovery
overlay. `review.rs` and `settlement.rs` call public library APIs from standalone ordinary
consumers. No production helper, `verified.rs` predicate or model is used as the reference oracle.

## Product finding conservation

The implementation surface inspected is `peritus-review`'s manifest and public exports plus
`src/product/{mod,error,finding,ledger}.rs`. Current callers inspected include the typed review
parser/serializer in `peritus-product-runner/src/review.rs`, fresh review admission in
`execution/review_phase.rs` and `execution/no_change.rs`, and fixer proposals in `execution/cycle.rs`. Supporting caller reads
are focused inspections, not complete review of those application subsystems.

The independent model indexes findings by a small logical `(category, title-key)` space. It
tracks original title and first review, latest payload/location/severity and observation cycle,
and an explicit Open / FixProposed / ResolutionConfirmed state. It compares every public field,
full history, unresolved projection, and blocker decision after every operation. Blocker policy
is an independent category/severity truth table, rather than a production blocking helper.

Review omission keeps untouched findings open. A fixer proposal keeps findings unresolved.
Only a later fresh review omitting a previously proposed finding confirms resolution. Repeated
findings reopen and update the observed payload without losing first admission or changing their
stable identity solely because location/severity/prose moved. Declared ASCII, accented Latin,
capital sharp-S and dotted-I alias pairs exercise trim/case normalization with exact expected
canonical titles. Expected identity framing is independently assembled; the SHA-256 primitive
is reused and is not itself under test.

The generator includes rejection of stale/skipped finding cycles and duplicate normalized
identities, positive review/fixer histories, reconstruction through public validated constructors,
and duplicate restore rows in varied order. Restore fixtures currently duplicate identical
logical records; ambiguous equal-cycle conflicting payload precedence is outside this pilot.
Reconstruction is a real public domain boundary, not application JSON/SQLite persistence.

Fixed cases cover every category/severity combination, location changes, identity aliases,
fixer proposal followed by omission and reappearance, rejection then continued use, and the
maximum cycle counter. Ordinary fixer-cycle values follow the current caller's positive cycle
contract. Invalid text sizes, malformed typed-review JSON, arbitrary fixer-cycle zero, and D2's
separate journal-backed aggregate/quorum/reconciliation machinery are outside this bounded lane.

### Confirmed exhaustion defect

On unmodified source, restoring an otherwise valid ledger at cycle `4294967295` and admitting
a submission at that same cycle returns `Ok(())` and changes its summary. The implementation
uses `saturating_add(1)`, so exhaustion admits a repeated cycle despite the documented rejection
of nonmonotonic reviews. This was reproduced through the public compiled API; the worker did
not infer it solely from code. `Case::ExhaustedCycle` deliberately expects rejection and remains
failing on the original source until the focused production fix is integrated.

The focused repair/regression lives separately on `fix/review-cycle-exhaustion`. The two live
review callers also used saturation, so both must reject an exhausted counter before invoking
`reviewer_turn::complete`; otherwise they spend a provider call before ledger rejection. The
deterministic public ledger replay admits `MAX-1 → MAX`, then requires the repeated `MAX` to
reject while preserving the full ledger. A live provider-call-count check is not established by
that domain replay. The bug must not be hidden by removing this discovery case or teaching the
model to allow saturation.

`calibration/stage3-review.patch` independently weakens omission conservation, allowing a reviewer
to resolve an untouched open finding. The calibration witness admits one high correctness
finding and then an empty fresh review, with no fixer. The expected finding remains Open and
blocking; the mutation resolves it. This canary is distinct from the genuine exhaustion defect.

## Candidate qualification and terminal settlement

The source surface inspected is the `peritus-run-settlement` manifest/public API and
`src/{identity,checkpoint,evidence,observation,reducer,settlement,stage,disposition,cause,error}.rs`.
Current application callers are candidate capture/refresh in `execution/checkpoint.rs` and
terminal construction in `execution/settlement.rs`. These callers explain where already-observed
facts originate; this pure pilot does not claim to verify filesystem observation or final UI text.

The independent model contains a selected raw candidate, the last admitted candidate checkpoint,
retained evidence provenance, and an optional terminal decision. It explicitly compares run,
workspace, content, public requirements, execution context and checkpoint sequence. Repository
history fences are distinct from evidence dependencies: changing repository history alone does
not invalidate source evidence. Gate records require matching observed execution context;
`None` on both sides is not an observed matching execution context. Obligation and review records
depend on source and public requirements without requiring execution context.

The model independently checks strict construction and reconciling observation. Missing,
Current, Failed and Stale statuses, positive/negative payloads, future records and each binding
axis are generated separately. Reconciliation retains original provenance, marks mismatches
stale, permits exact reversion to recover matching evidence, and rejects matching future records.
A finite truth table bounds the requested qualification stage by current gate, obligation and
review evidence. Negative or changed observations may downgrade qualification at a later sequence,
including unchanged source content. A checkpoint must advance its sequence in the same lineage;
failed construction/admission preserves the prior reducer state and later operations continue.

The terminal table tests all 13 public settlement causes, with and without a qualified candidate.
Cancellation, wait and recovery retain their explicit dispositions; acceptance requires the
qualified stage and all evidence. No-change completion has its own documented disposition.
Repeated settlements and post-terminal observations reject. Explicit restart operations create
a **new** reducer instance, optionally observing the retained candidate; they do not claim the
same terminal reducer can resume. Clearing execution context is separately modeled so later gate
currentness can be checked. This is public pure-domain restart composition, not process recovery.

Fixed examples cover all terminal causes, the full requested-stage/three-evidence truth table,
strict and reconciled future/stale/failed cases, sequence and lineage rejection, candidate axis
changes and exact reversions. Metrics count actual accepted checkpoints, qualification/downgrade,
constructor and transition rejection, evidence probes, terminal dispositions, repeated terminal
rejection, new reducer instances and independent state comparisons.

`calibration/stage3-settlement.patch` makes cancellation override apply only to unqualified
candidates. A previously qualified candidate then falls through to Accepted during cancellation.
The fixed witness qualifies a candidate and cancels; the independent terminal table must require
Cancelled with that candidate retained and unaccepted.

The first calibration reached qualification and cancellation independently but failed to combine
them in its generated histories. It remains recorded as incomplete. The strengthened strategy
keeps broad mixed histories and gives one quarter of cases a constructive interaction: arbitrary
prefix, fresh reducer, varied content and execution binding, strict or reconciled full
qualification, successful reobservation, cancellation, arbitrary continuation. This family stays
within the requested operation bound and preserves the serialized operation schema. Actual-state
metrics separately count terminal attempts after qualification and qualified cancellations;
neither is inferred by adding unrelated event totals. Final calibration requires generated
detection under two seeds as well as the explicit witness and restored replay.

## Boundaries and execution evidence

The product finding ledger itself carries no candidate identity or gate/obligation evidence.
Those bindings belong to the settlement and application acceptance composition; the pilot does
not invent candidate binding inside the ledger API. Scheduler ownership, D2 durable replay,
gate execution, obligation extraction, real providers, archive persistence, process crashes,
concurrent callbacks and actual product resume are separate campaigns.

### Early isolated qualification

The worker ran an ordinary nested consumer against the coordinating checkout's public library
sources after the focused review-cycle fix was integrated. The final bounded snapshot is
`stage3-independent/evidence/early-02` in this lane's `discovery/proptest-praxis` folder. Exact
commands, separate stdout/stderr, exit statuses, inspected source SHA-256 maps, fixed case JSON
and per-target metrics are retained there. The recorded owner/module files had identical hashes
before and after the campaign. This is independent early evidence, not the final root runner's
source fingerprint or qualification result.

Both targets passed with seed `19427`, ChaCha and a 64-step generator bound. Review passed all
five fixed cases and 1,000 generated cases: 949 operation histories plus 51 atomic exhausted-cycle
witnesses, totaling 29,541 history operations. Settlement passed all 30 fixed cases and 1,000
generated histories totaling 31,851 operations. The review search reached 11,873 accepted reviews,
39,630 resolution confirmations, 18,671 reopenings and 4,386 validated reconstructions. Settlement
reached 5,134 accepted checkpoints, 52 qualified checkpoints, 1,050 qualification downgrades,
8,609 construction rejections, 6,040 transition rejections and 3,383 new reducer instances.

Strict isolated `cargo clippy --offline --locked --all-targets --all-features --no-deps -- -D warnings`
passed for the two modules and the minimal consumer. The first lint pass found one harness-only
`manual_is_multiple_of` finding; the final source uses `is_multiple_of(2)` and the complete campaign
was repeated successfully after that change. No SDK mutation was made during this qualification.

No calibration mutation or final qualification is established by this document's presence. The
coordinating runner separately records final source digests, exact cases, commands, generator
metrics, failure artifacts and baseline/mutant/restored evidence.

The separate early evidence directory is also preserved in the coordinating snapshot at
`runs/stage3-independent-early-02/`. Its older absolute source/command paths remain provenance;
it does not substitute for the final exact-source reports selected by `evidence/stage3.json`.
