# Obligation binding, failure ownership and performance thresholds

A public improvement requirement incorrectly accepted arbitrary regressions when the admitted
noise margin was at least the requested improvement. For baseline 100, candidate 1000, required
improvement 5 and noise 10, the old saturating subtraction discarded the negative improvement
and qualification returned success. A test using the public qualification API fails on the exact
base commit with only the new fixture added.

The root-cause correction compares baseline plus noise against candidate plus required
improvement using u128 sums, which cannot overflow for two u64 operands. All four threshold
variants now prove equivalence to independent unbounded-integer inequalities. The actual
PerformanceEvidence::satisfies method additionally proves exact workload bytes, statistic,
minimum repetitions and copied threshold agreement. Constructor/getter and semantic Clone
contracts retain the complete measured inputs. Binding currentness is checked separately by
the production evidence verdict before type satisfaction; it is not part of satisfies itself.

EvidenceBinding now proves exact successful bounded canonical path admission, all-byte requirement
and ledger currentness, candidate/sequence currentness, and full path membership including early
lookup termination. A private type invariant connects constructor-enforced ordering to global
uniqueness. Its actual Clone preserves every field and path. Failure disposition is proved exact
for the supplied owner, clarification-use and recovery-availability facts; only CandidateDefect
selects RequestFixer. Typed obligation identities retain exact digest views.

Independent Sol source review checked all 11 source hashes, specifications, actual callers,
behavior preservation and proof nonvacuity. The reviewer inspected the implementation-produced
logs and did not independently execute those commands. Integration first verified every shared
preimage against the exact base and then copied only the reviewed hashes. Isolated and integrated
strict pinned Verus each passed 192 checks with zero errors; all 16 package tests passed in both.
Strict all-target Clippy and formatting passed. Restoring only the old improvement expression in
a separate fixed-source copy fails the threshold postcondition at 191 verified and one error.
This is a local negative proof case, not a complete hosted CI enforcement demonstration.

The qualification traversal, other typed evidence contracts and 13 existing non-Copy Clone
warnings remain open proof work. Observations are supplied inputs: the proof does not establish
measurement truth, workload authenticity, noise-policy selection, hashing correctness or effects.
No registered obligation is discharged here. These records are independently reviewed local
source evidence, not protected review authorization or final clean-commit CI attestations.
