# Exact obligation values and requirement admission

An independent Sol source review accepted the nine-file foundation increment. The parent verified
the exact isolated source with 225 strict Verus results and zero errors, then integrated those nine
files after confirming the shared package still matched the preimage. Integrated verification
reproduced 225 results and all 25 integration tests. Both default and all-feature isolated tests,
strict all-target/all-feature Clippy and formatting passed.

## Proved production behavior

Errors expose their exact category, optional requirement ID and numeric details. Limits admission
is an exact equivalence, including rejection of each zero bound and clause bounds larger than the
source bound; successful construction and the production defaults retain all six bounds. Condition
observations preserve their exact identity, truth state and supplied observation digest.

The production source `from_parts` constructor admits exactly nonempty, bounded content and retains
every source byte, the supplied digest and conversation revision. Clause constructors/getters retain
the complete source digest, revision, ordinal, byte span and exact clause bytes. These contracts do
not assert that the supplied digest was cryptographically computed from those bytes.

Path construction preserves exact identity, spelling and role with exact size admission and error
details. Candidate evidence is required exactly for output and modification roles. Actual path
validation checks the size first, then the first non-increasing adjacent identity pair, using every
byte of each 32-byte identity. It proves exact successful admission, global uniqueness and exact
first-error content. Replacing derived ordering with verified byte comparison preserves the prior
lexicographic ordering and error priority.

Requirement specification classification, condition/group selection and schema-direction validation
have exact contracts. `RequirementEntry::new` admits exactly a valid specification and bounded,
strictly ordered path sequence; shape errors precede path errors. It retains all supplied fields and
establishes a private shape/canonical-path invariant. The actual ledger extraction still calls this
constructor after creating exact clause bytes and provenance.

Five actual Clone implementations now preserve all source, clause, path, specification and entry
content, including all nested schema fields and path roles. No obligation-package Clone warning
remains in this checkpoint; two settlement dependency warnings remain. The private specification
validator is now const after its equivalent direction match became const-compatible.

## Negative and ordinary evidence

A controlled change replacing only a cloned path's role with `Reference` failed the complete-content
postcondition: 224 verified results and one proof error. Restoring the original source reproduced
225 verified results and zero errors. The retained mutation record includes the exact command and
original/mutated source hashes.

Four public regression cases exercise size-before-order and first-pair error precedence, full-byte
identity ordering, binary source/path content, all five roles, clause provenance and cloning,
condition details, requirement-identity errors, each zero limit and source-bound details. Existing
performance false-positive and schema regressions continue to pass.

## Remaining work

Whole ledger extraction and alternative topology, canonical serialization/hashing, qualification
traversal and its report are not yet covered by these contracts. The source hash computation remains
an ordinary caller of the verified constructor. No theorem binds a `PathId` to the spelling bytes or
establishes external observation authenticity. These boundaries require their own correspondence and
technical assessment; this checkpoint does not exclude feasible follow-up work.

The independent review inspected the exact source, production callers and supplied logs; it did not
independently execute Cargo. Parent isolated and integrated checks are identified separately. No
new runtime guard, public execution precondition, assumption, proof escape or lint suppression was
introduced. This checkpoint is not an obligation discharge or final repository/hosted qualification.
