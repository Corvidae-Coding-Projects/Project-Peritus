# Independent review: run-knowledge delta material helper/model increment

Review date: 2026-09-12. Review mode: defensive, read-only source and proof-log inspection. No source was edited and no compiler/test/lint command was run by this reviewer.

## Provenance and scope

- Frozen package root: `/tmp/peritus-sol-knowledge-delta-parent-leaf-preimage-01`.
- Reviewed six-file manifest: `/tmp/peritus-parent-knowledge-delta-leaf-review.sha256`, SHA-256 `331411026f0fe75f181f52e0335ff364152645717b2bd7336238581e3d4c280f`.
- `sha256sum -c` passed for all six paths: `src/kind.rs`, `src/section.rs`, `src/model.rs`, `src/model/material.rs`, `src/delta/material.rs`, and `src/delta.rs`.
- `diff -qr` against `/tmp/peritus-parent-knowledge-delta-preimage/crates/orchestration/peritus-run-knowledge` showed exactly the four modified files and two new helper files named by the manifest; no unlisted package drift was observed.
- Supplied compiler log `/tmp/peritus-parent-knowledge-delta-verus1.log` has SHA-256 `5df4505a35e651f3912144230bd5a5cb2b907c79ba5297b3e7c0022cb3b24d66` and ends with `verification results:: 182 verified, 0 errors`. The log contains dependency/trigger warnings and does not print the invoking command, so this review records the result as supplied provenance rather than an independently reproduced strict invocation. No partial-increment tests, Clippy, or format evidence was claimed or inferred.

## Bounded verdict

PASS for the six-file helper/model increment. I found no correctness, specification, runtime-equivalence, or escape blocker in the reviewed scope.

`KnowledgeSectionKind::spec_authority` is an exhaustive semantic classification of the eight variants, and the production `authority` match is contractually equal to it. `KnowledgeSection::authority` and `can_satisfy_authoritative_evidence` carry that exact classification through the already-verified authority gate. The change preserves the existing public executable result and does not add a guard or precondition.

`section_material_matches` represents exactly the fields compared by the preimage's executable `same_material`: kind, all 32 section-digest bytes, ordered source entries, and ordered dependency identities. `sources_match` requires equal lengths and pointwise source identity plus all content-digest bytes. `dependencies_match` requires equal lengths and pointwise equality of every 16-byte section identity. These are order-sensitive sequence predicates; they do not collapse either collection into a set. The production helpers traverse left-to-right, reject length mismatch first, and have exact iff postconditions. `KnowledgeSectionKind::matches`, `SourceDigest::matches`, `KnowledgeSectionId::matches`, and `identity::bytes_equal` each already have exact equality contracts, so replacing derived equality with these helpers preserves the preimage's pure runtime truth value and asymptotic behavior.

`first_prior_material_matches` precisely characterizes the first matching section identity: a witness must match the current 16-byte ID, no earlier element may match, and that witness must satisfy the complete material predicate. The production `prior_material_matches` performs that same first-ID traversal. Its false proof covers all alternatives: an earlier candidate contradicts the loop invariant, a later candidate cannot be first once the current match exists, and the current candidate failed material equality. If no identity is found, the loop invariant establishes absence. This remains exact even without relying on snapshot uniqueness; canonical snapshots independently guarantee uniqueness.

The production call graph is real rather than a detached Boolean theorem: `plan_delta_packet` calls the exact-contract `prior_material_matches`, which calls the exact-contract `same_material`; its navigation branch also calls the newly exact authority accessor. The existing `prior_plan.is_reused(section.id())` remains a separate required conjunct before `CurrentReference`, so material equality alone cannot admit stale prior knowledge. External `peritus-context` code consumes the resulting public packet and independently checks section digest and delivery/authority compatibility.

## Deliberate and open boundaries

This is not yet a full `DeltaPacket` refinement. `plan_delta_packet` has no postcondition tying every emitted entry to the declarative navigation / prior-reuse-and-first-material-match / changed-fact partition, and `DeltaPacket`, `DeltaEntry`, and delta accounting lack the complete logical views and exact output/count contracts needed for that theorem. Its role/candidate/current-snapshot error paths and the prior-plan result are not newly characterized by this six-file increment. Those belong to the in-progress full packet work.

Material equality deliberately excludes section ID, binding candidate, binding role, and binding creation sequence. ID selection happens in the outer first-match lookup; lineage, role, source currentness, sequence, revisions, and dependency invalidation are enforced through `plan_invalidation` before `CurrentReference`. Therefore this helper must not be described as full section equality or as a standalone reuse authorization.

The source and section digests are exact caller-supplied byte values. These proofs do not establish that an external file was hashed authentically, that observations are fresh outside the supplied state, that I/O occurred, or that a provider rendered the resulting packet. The read-only caller inspection establishes a structural production path, not a verified downstream context-rendering theorem.

No assumption, admit, axiom, `external_body`, alternate non-Verus implementation, new executable precondition, or suppression was found in the six reviewed files.
