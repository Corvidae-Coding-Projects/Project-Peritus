# Security requirement and criterion input checkpoint

The production requirement and acceptance-criterion traversals now have exact predicates
over their supplied observations. Each catalog entry uses the first matching observation
for the exact candidate, preserving existing lookup and diagnostic order. Admission requires
a Passed outcome and a nonzero evidence digest. Missing observations, failed outcomes, and
empty digests retain their existing diagnostics.

The lookup contracts establish first-match identity and absence. Uniqueness lemmas connect
that concrete first index to the input-defined admission predicate. Loop induction establishes
the complete catalog conjunction, and successful traversal preserves the initial diagnostic
sequence. Candidate equality is the full previously reviewed candidate identity relation.

At this historical five-file checkpoint, top-level Ready implies complete requirements and
criteria. It does not yet establish the full inventory, artifact, review, and finding
conjunction. A subsequent increment is being reviewed separately. Neither checkpoint
authenticates the supplied outcome, digest contents, native execution, or reviewer identity.

Pinned strict Verus passed 174 checks with zero errors. Parent restored the five exact files
over the independently retained readiness source checkpoint and reproduced that result and
all seven package tests. Implementer all-target strict Clippy, formatting, source-layout,
and forbidden-pattern checks passed. Two earlier test attempts failed during linking when
temporary filesystem and memory resources were exhausted; the disk-backed retry passed.

Parent review checked first-match behavior, complete catalog coverage, exact outcome/digest
predicates, actual production composition, and source hashes. This is bounded source review,
not an obligation discharge or protected review authorization. Source manifests are not
complete compiler-input attestations or final clean-commit evidence.
