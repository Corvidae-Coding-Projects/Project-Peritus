# Exact production knowledge constructor admission

This increment connects the existing production constructors to complete input-defined admission
and error contracts. It strengthens successful field retention into `Ok` if and only if the actual
accepted shape holds, and establishes exact error categories, optional identities and numeric
payloads with the existing first-error precedence. The frozen package contains 37 files; 18 files
changed from the separately reviewed 214-result delta checkpoint. This is a bounded proof increment,
not an obligation discharge or final hosted CI qualification.

## Proved production behavior

`KnowledgeLimits::new` succeeds exactly when all four configured limits are nonzero and retains all
four supplied values. Both identity constructors succeed exactly when some byte is nonzero, retain
all sixteen bytes, and return the exact category-only zero-identity error otherwise. Every
`KnowledgeError` constructor and getter has a complete field contract.

Source admission checks required emptiness before maximum length, then the first noncanonical
adjacent source identity. Exact comparison covers every identity byte. Its error retains the
current rejected source identity, with every unrelated detail absent. `KnowledgeBinding::new`
checks supported role first, nonzero creation sequence second, then this source admission under
the caller's per-section source limit. `CurrentKnowledgeState::new` uses the caller's catalog
limit. Both constructors retain all supplied candidate/source fields, and their intrinsic type
invariants and actual semantic Clones retain the admitted shape.

`KnowledgeSection::new` checks dependency count first, then each dependency in order, checking
self-reference before that position's duplicate or descending-pair error. The shared production
identity validator preserves that order with an optional excluded section identity; request
validation supplies no exclusion. `InvalidationRequest::new` requires nonempty targets exactly for
UserClarification, rejects targets for every other change, and then checks canonical target order.
Its exact state, change and target fields are retained. Section and request type invariants and
Clones preserve these intrinsic shapes.

`RunKnowledgeSnapshot::new` checks supported role, the three-section minimum, the caller's section
maximum, then each section's canonical identity order, candidate lineage, role, creation sequence
and backward dependency membership. Required references are checked last against their exact
RepositoryInventory, RelevantFileMap and LiteralRequirementLedger kinds. The full constructor
now proves exact admission and the first failure with its complete detail. A strengthened lookup
contract identifies the first matching section; it establishes that rejecting a dependency which
does not resolve before its consumer is complete even before the whole input has been validated.
The snapshot retains all supplied fields and a full admission/topology invariant. Its actual Clone
preserves all binding facts as well as the existing global-unique backward topology.

## Preserved policy boundaries

Binding construction does not compare creation sequence to its own supplied candidate checkpoint.
Snapshot construction compares section creation against the supplied snapshot checkpoint. Snapshot
lineage means the same run and workspace; different candidate digest or conversation revision is
allowed by this constructor. Later freshness/reuse planning still applies its stronger checks.

Each nested binding/section constructor applies its own allocation limit. A snapshot applies its
section count limit and does not reapply its other limits to already admitted nested values. The
proofs and regression tests explicitly preserve those rules. They do not silently add guards or
claim stricter behavior than production implements.

Derived byte ordering/equality was replaced where needed by exact existing byte comparison
semantics, and role/clarification comparisons use their verified exhaustive predicates. The
shared collection validator is called by the actual constructors. There is no alternate
verification-only executable implementation, new public executable precondition, assumption,
axiom, external body, lint suppression, or weakened repository checker in this increment.

## Validation and limitations

The pinned strict root command with `--no-cheating --rlimit 20` passed 216 verification results and
zero errors. Both default and all-feature ordinary suites passed 29 tests. Nine new public tests
cover every limit, all role/change variants, late-byte ordering at each of sixteen byte positions,
exact first-error details, complete field/Clone retention, and the preserved checkpoint/nested-bound
policies. Strict all-target all-feature Clippy and package formatting passed.

An isolated mutation changed the actual descending-source error detail from the rejected current
identity to the previous identity without changing specifications. Verus rejected it with 215
verified results and one failed exact-error assertion. Restoring the exact source passed 216/0.
The mutation, source identities, command, failure log and restored run are retained.

The current shared xtask checker was built and run in the isolated source workspace: ordinary API
passed for 3387 formal-boundary files and 14618 ordinary-safe entries, and source layout passed for
4273 files. These numbers describe the isolated workspace, not final shared source or hosted jobs.
A separate current shared scan passed ordinary API for 3474 files/14676 entries and source layout
for 4375 files while other agent work continued; it is also an interim scan.

Supplied source/content digests, candidate identities and observations still rely on their
observing boundaries. This increment proves constructor behavior, not hash authenticity, external
execution, persistence or end-to-end product resume. Exact error payloads for the public planners
remain separate work from the constructor errors proved here. Complete context selection/rendering
and other already identified gaps remain open. Existing dependency Clone warnings and automatic
trigger notes remain visible in raw logs; this is not a warning-free workspace claim.

Independent Sol source review passed for the frozen 18-file increment and checked both source
manifests. The reviewer inspected supplied logs without independently rerunning compiler commands.
The parent integrated only the reviewed paths after checking the exact 30-file preimage. All 37
resulting package hashes matched. Integrated strict Verus passed 216/0, all 29 package tests passed,
and both existing context-reuse integration tests passed. The original reviewed draft is retained
separately so its identity still matches the independent review record. No obligation status or
final proof inventory is discharged by this bounded increment.
