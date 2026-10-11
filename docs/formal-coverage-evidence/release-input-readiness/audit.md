# Release input-to-verdict correspondence

Parent independently reviewed the twenty changed source files and their production call chain,
then restored the exact full package in an isolated tree and reproduced 336 strict pinned Verus
checks with zero errors and all 29 tests (28 executable and one compile-fail doctest). Implementer
strict all-target/all-feature Clippy and package formatting passed. Changed files are below 400
lines; no full repository layout pass is claimed at this checkpoint.

## Bounded proved result

The actual artifact traversal agrees with a finite operational fold over supplied observations.
For each of the 44 closed requirements, the fold accounts for currentness, source class, supplied
review/signing flags, saturated category counts and conflicts among contributing digest bytes.
The fixed requirement array reaches the canonical criterion reducer, whose conjunction agrees
with the 25-criterion input model. Qualification, review, finding and waiver folds retain their
previous exact input connections.

Diagnostic append operations prove nondecreasing length and no-growth iff the corresponding
assessment's diagnostic condition is clear. Their composition establishes that final diagnostic
emptiness, the raw stored Ready verdict and is_ready each hold iff the complete supplied-input
fold conjunction holds. The candidate, evaluation tick and aggregate artifact, criterion,
qualification, review and finding readiness flags also correspond exactly. These guarantees
reach production evaluate_release and its ReleaseDecision constructor.

EvidenceBinding construction now has exact success/admission and stored-field contracts.
ReleaseEvidence construction has exact collection-limit admission and stored collections.
Successful artifact, qualification, review, finding and waiver observation construction preserves
all supplied fields. These are admission/representation facts, not authentication facts.

## Review distinctions and remaining feasible proof work

The input predicate is defined by exact finite operational folds. A separate quantified
all-observation/quorum characterization remains open. The criterion-array contract currently
proves conjunction equivalence, not each canonical slot's identity and individual value. The
hard-coded slot mapping matches the current catalog by source inspection; no runtime mismatch
was reproduced. Complete per-slot output contracts through the public decision, exact saturated
output counts, aggregate and decision digests, full diagnostic contents/order, and the remaining
observation constructors' Ok-iff guards still require proof. Nondecreasing diagnostic length alone
does not prove full prefix preservation or exact diagnostic content.

Parent raised these distinctions with the implementer, who acknowledged them and kept the source
frozen. This review accepts the stated input-to-verdict increment and creates no full obligation
discharge, proof exclusion or claim that all release-policy verification is complete.

Observation authenticity, cryptographic verification, signer-registry admission, native execution,
I/O and truthful producer reports are outside this pure supplied-input theorem. The evaluator
performs no publication operation and grants no tagging, signing, upload or deployment authority.
