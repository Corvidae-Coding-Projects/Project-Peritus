# Kernel waiver request and grant correspondence

The production GrantWaiver helper now proves that a successful grant uses the stored Requested
waiver, its exact review-cycle and run binding, and current supplied policy authority for that
finding. The public reducer carries the authorization predicate from its original input
aggregate to the granted result and emitted waiver subject. The helper preserves the complete
working state on rejection; the outer reducer returns its original aggregate on failure.

A baseline regression reproduced a real defect: a waiver request for one review cycle could
be granted using a finding from another current review cycle. The old lookup searched all
current reviews. The repaired lookup requires the finding's stored request review and current
revision. The regression now rejects with AuthorityMismatch and leaves the request Requested.
Stored review/run consistency also rejects malformed aggregate input; normal aggregate
preflight already requires that consistency.

Exact Clone contracts close the connection between the public reducer's input and its working
copy. They preserve all aggregate scalars and ordered sequence contents, every action field,
optional authorization witnesses, and both capability text and byte views. The reducer's
previous redundant scalar rewrites are removed. KernelCommand contains only Copy payloads
and now derives Copy, allowing its generated exact Clone specification. No new warning
suppression, public execution precondition, assumption, or verification-only body was added.

Independent parent review inspected the waiver helper, lookup, input predicates, public
propagation, complete clone-field correspondence, all 22 final source identities, and regression
evidence. Strict pinned verification passed 295 checks with zero errors in isolated and
integrated sources; the final kernel root emitted no warnings. The isolated clone suite
passed 14 kernel tests. The final integrated source includes the additional waiver regression
and passed 15 kernel plus 40 quality-policy tests. Isolated all-target strict Clippy and
formatting passed; downstream checks prompted by the Copy API addition are tracked separately.

The public guarantee is conditional on a GrantWaiver command emitting WaiverGranted. It does
not yet prove complete lifecycle admission/rejection equivalence, projection correctness,
the absence of a grant event from every other command family, or external evidence and
authority authenticity. This is an independently reviewed bounded implementation checkpoint,
not a whole obligation discharge, protected approval, complete compiler-input attestation,
or final clean-commit result.
