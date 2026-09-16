# Directional schema admission and coverage checkpoint

The parent independently reviewed the Sol implementation against the eight exact source
identities in `source-sha256.txt`, the preimage diff, and the production requirement and
qualification callers. Independent verification of the isolated full package reproduced
222 verified results with zero errors and all 21 integration tests passing.

## Proved production contracts

`SchemaField::new` succeeds exactly when its supplied name is nonempty and within the supplied
bound. Success preserves the field identity and every name byte. `SchemaRequirement::new`
succeeds exactly when its field sequence is nonempty, within the configured bound, and strictly
ordered by the complete 32-byte field identities. `SchemaEvidence::new` establishes the same
bound and ordering conditions for observations, while permitting an empty observation sequence.
Successful construction preserves the supplied direction, sequence and evidence binding.

Strict ordering implies global identity uniqueness. Private type invariants preserve the
canonical properties needed by the actual linear two-cursor coverage algorithm. Its result is
equivalent to matching the stored direction and finding every required field identity in the
observed identities. The quantified specification permits observed supersets and compares all
identity bytes. There are no new public execution preconditions or rejection guards.

The actual clones of fields, requirements and observations preserve their complete semantic
content. The actual `RequirementEvidence` clone preserves the variant and all stored fields in
all six evidence variants. Narrow crate-private order lemmas compose previously verified byte
comparison properties; the limits getter now exposes the actual stored schema bound.

## Production correspondence and tests

`RequirementEntry::new` still calls the direction-shape validator before validating paths.
The ordinary qualification reducer calls `SchemaEvidence::covers` for request and response
requirements after current-binding and required-path checks. That call connection is inspected
source correspondence; this checkpoint does not prove the whole qualification reducer.

The five new tests cover empty and oversized names, exact-bound binary names, size-before-order
error precedence, duplicates, descending and last-byte identity order, empty observations,
supersets, missing middle and terminal fields, opposite directions, clone equality, and ledger
rejection of both mismatched schema variants. Existing performance and evidence regressions
also passed. Implementation default and all-feature tests each passed 21 cases; the independent
all-feature run passed the same 21 cases. Implementation strict Clippy and formatting passed.
The recorded source-layout run passed for its then-current 4,344 source files.

## Review limits and remaining work

Constructor postconditions establish exact admission and successful values, but do not model
every error value and its precedence. Those details are regression-tested and remain feasible
proof work. The intrinsic field invariant retains nonemptiness; the caller's discarded bound
is established at construction rather than retained as a lifetime invariant.

Full qualification traversal, conditions, alternatives, report accounting, ledger/provenance
contracts and their remaining semantic clones are not discharged here. Five existing obligation
Clone warnings remain in path, provenance, obligation specification and requirement entry types;
the verification log also records two settlement dependency Clone warnings. Supplied observation
truth, external execution and cryptographic authenticity remain outside this schema theorem.

The diff introduces no assumptions, axioms, external bodies or lint suppressions. Existing
file-level documentation allowances in schema and evidence predate this increment. No new runtime
bug was reproduced in this schema increment. This is a reviewed source checkpoint, not a formal
register discharge, final repository qualification, hosted CI result or external approval.
