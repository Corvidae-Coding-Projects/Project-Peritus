# Candidate and settlement domain checkpoint

The actual `CandidateIdentity`, `CandidateCheckpoint`, and `SettlementReducer` implementations
now have exact contracts for identity comparison, evidence freshness, checkpoint admission,
terminal disposition, and state preservation on rejection. Production recorder, restore,
daemon persistence, and wire decoding paths use these domain constructors and operations.

Candidate comparison includes every run, workspace, and digest byte plus conversation revision.
Checkpoint sequence is deliberately excluded from candidate identity; current evidence must
also have an observation sequence no later than the candidate checkpoint. The constructor
checks current evidence binding, stale evidence binding, then stage support, with exact
rejection priority. Successor admission checks lineage, strict sequence advancement, then
stage monotonicity for an unchanged candidate.

`observe` succeeds exactly when the reducer is unsettled and the successor is admissible.
It replaces the checkpoint and preserves the terminal field; every rejection preserves the
complete reducer. `settle` succeeds exactly once and retains the supplied cause and latest
checkpoint. User wait, cancellation, and recovery take precedence over qualification. For
every other cause, a qualified checkpoint produces Accepted, another checkpoint produces
CandidateAvailable, and no checkpoint produces FailedNoCandidate. This preserves existing
behavior; acceptance is not restricted to the Completed cause.

The only executable changes are an extensionally equal byte comparison and an equivalent
enum match needed to expose their exact semantics to Verus. No new public execution
preconditions, verification-only implementation, assumptions, or proof escapes were added.
The underlying evidence facts, digest contents, external clocks, effects, durable writes,
and recovery orchestration remain outside this domain proof. Stable tag conversions retain
their existing ordinary round-trip tests. Two pre-existing generic evidence Clone warnings
remain; these mutators use Copy and no source or test in this crate invokes Clone.

Pinned strict Verus passed 71 checks with zero errors in both the isolated implementation
and integration worktree. All nine ordinary tests passed in both, including full rejection
state preservation, all identity-byte positions, and qualification overridden by cancellation,
recovery, or waiting. Strict all-target Clippy and formatting passed in the isolated source.
Independent Sol reviewer `/root/sol_acceptance_completion` checked all nine source hashes,
exact specification and execution correspondence, nonvacuity, actual callers, and these limits.

This accepts the bounded domain increment, not a whole registered obligation or authenticated
CI approval. The source manifest is an implementation checkpoint, not a final clean commit
or complete compiler-input attestation.
