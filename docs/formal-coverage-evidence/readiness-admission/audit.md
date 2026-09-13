# Release and security admission checkpoint

The actual release evaluator's Ready result now implies a supplied contributor for each
of the 44 required evidence kinds. Each contributor must have the required source kind,
supplied signed/reviewed flags, current observation window, producing revision, and complete
candidate identity. The identity comparison checks the nine top-level components, including
every nested Git, version, platform, toolchain, profile, and schema component. This replaces
the specification's earlier reliance on the manifest digest alone. The production comparison
and current/mismatched/stale predicates now have exact contracts. Runtime evidence validation
retains its existing branch order; no duplicate acceptance guard is used to obtain the proof.

The release decision constructor now preserves the supplied candidate, time, assessments and
diagnostics, and its Ready result is equivalent to the complete assessment condition. The
security decision constructor likewise has an exact result/checks/diagnostics contract.
These constructor theorems alone do not prove the semantics of every input assessment.

Production qualification calls two verified finite reductions. H0 reduces the actual outcome
enum for all 42 canonical probes together with the security decision. The existing ordinary
run constructor enforces the fixed catalog order and unique subject identities; the report
passes those outcomes to the kernel and the JSON encoder checks the report's final admission.
H4 passes seven local validation results and one adapter-input result to the eight-check
reduction with the release decision. Its operator now calls `evaluate_verified`. The generic
adapter API remains available and is documented as reporting the configured adapter's verdict.
Missing evidence still prevents policy invocation, and failed checks retain their blockers.

Independent parent review checked all 33 source identities and read the changed implementations,
input/output contracts, candidate comparator, canonical outcome mapping, local validators,
adapter and operator calls, and scenario tests. Restoring those exact files over base commit
`1aa9282ff2cfcbf8fac325f11f157f408d33ee30` in an isolated source copy reproduced 82 passing
ordinary tests and strict Verus results of 304 release-policy and 164 security-policy checks,
all with zero errors. The implementation also passed four-package strict Clippy and formatting.

This is an independently reviewed bounded increment, not full H0/H4 input equivalence or an
obligation discharge. The ordinary validators, outcome classification, serialization, adapter
translations, evidence authenticity, signatures and external review provenance are not proved
by the finite reductions. Release qualification/review/finding traversals and security input
checks need further correspondence work. No unsupported-operation exclusion is justified by
the qualification crates' ordinary-Rust classification alone. The release staging workflow's
unpublished-draft behavior is unchanged.

The implementation froze this checkpoint at 2026-09-12T19:19:40Z. Subsequent security-policy
work can supersede some source hashes. The manifests identify this increment and its raw output,
not a complete compiler-input attestation or the final clean CI commit.
