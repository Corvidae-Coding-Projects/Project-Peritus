# Snapshot topology and transitive invalidation

The parent independently reviewed all ten changed source files against the earlier knowledge
checkpoint, including the complete 1,268-line diff, actual constructor and planner contracts,
semantic clone fields, and delta/product-resume call sites. This bounded review accepts the
constructor-to-planner topology connection; it does not discharge a whole registered obligation.

## Proved

A successfully constructed RunKnowledgeSnapshot retains every supplied field and establishes
strict order over every identity byte, global identity uniqueness, a prior indexed section for
every declared dependency, and the three exact required section kinds. The private type invariant
retains those facts through construction and complete semantic Clone. No ordinary executable
precondition is added.

The actual plan_invalidation body returns one exact decision per input section and complete
accounting, as in the earlier checkpoint. Its strengthened postcondition now also proves
invalidation closure over the recursively defined finite backward dependency relation. Constructor
admission ensures every declared dependency has such a backward edge, so forward references are
not silently omitted by the relation. The public decision getter returns the exact value,
including the invalidation reason. Direct decision precedence remains unchanged.

The internal lookup now scans all identities. During dependency admission, an accepted match must
still precede the current section, whose earlier prefix has already passed canonical order checks.
Required-kind lookup runs only after all sections pass canonical validation. The removal of the
old early exit therefore preserves these callers' acceptance and error behavior. No runtime
regression was reproduced by this increment.

## Tested and independently verified

The implementer and parent each obtained 178 strict pinned Verus checks with zero errors. The
implementer ran default and all-feature tests, 16 passing in each configuration; the parent
independently ran all 16 all-feature tests. Strict all-target/all-feature Clippy and formatting
passed. The retained source-layout attempt failed on two separately owned in-progress release
and scheduler files; it is not a repository-wide pass. One existing derived-Clone warning in
knowledge delta remains; dependency warnings are separate.

## Remaining limits

This theorem covers admitted snapshots and successful planner results. It does not prove arbitrary
unadmitted graphs, source observation authenticity, complete constructor admission iff conditions,
lineage/time/bounds invariants, delta composition, context selection, persistence or the end-to-end
product resume path. Those call sites were inspected to establish use of the actual proved planner,
not to claim their own verification. No exclusion or proof discharge is created by this review.
