# H4 verified release policy

H4 is the final deterministic production-readiness reduction. It evaluates retained evidence for
one immutable release candidate and returns `Ready` or `NotReadyForProduction`. It performs no
effects and grants no publication authority.

The implementation lives in `peritus-release-policy`, a V-class foundation crate. Its executable
decision, runtime-facing types, specification predicates, and proof obligations are inside
`verus!`; there is no separate ceremonial proof implementation that can disagree with runtime
behavior.

## Exact release identity

`ReleaseCandidate` is private-field, checked data. It binds:

- a nonzero nominal candidate ID;
- the exact Git SHA-1 or SHA-256 commit object;
- semantic major/minor/patch values and a digest of the complete version text;
- exact Linux, macOS, and Windows OS/architecture/profile identities in fixed order;
- exact Rust, Verus, vstd, and solver digests;
- the runtime and qualification profile revision plus digest;
- policy, evidence, report, and artifact schema revisions plus the closed-catalog digest;
- the positive producing source revision; and
- the canonical release-manifest digest binding the complete external manifest.

All observations carry `EvidenceBinding`: the complete candidate, a monotonic observed/expiry
window, a positive source sequence, and a positive source revision. An observation contributes only
when the candidate and source revision are exact and the evaluation tick is inside the inclusive
validity window.

## Canonical 25-criterion catalog

The catalog preserves the architecture contract's stable IDs and meanings:

| ID | Stable name | Required evidence |
|---:|---|---|
| 1 | clean-tier-one-suite | Gate A; complete ordinary quality matrix on Linux, macOS, and Windows |
| 2 | verified-workspace-build | clean locked workspace Verus verification and verified release build |
| 3 | proof-obligation-inventory | complete proof inventory and approved narrow exclusions with compensating evidence |
| 4 | trusted-construct-audit | trust-boundary scan, allowlist, threat analysis, and refinement evidence |
| 5 | privileged-construction | ordinary-wrapper and privileged-state construction conformance |
| 6 | illegal-lifecycle-edges | Verus/property/protocol illegal-edge matrix |
| 7 | crash-recovery | journal/blob/snapshot/lease/patch/gate/promotion crash campaign |
| 8 | deterministic-replay | empty-projection byte-exact compatibility replay |
| 9 | malicious-repository | traversal, race, repository trick, alias, injection, output, terminal, and exfiltration suite |
| 10 | native-sandbox-security | Linux, macOS, and Windows native qualifications plus independent escape review |
| 11 | role-isolation | writer/reviewer/fixer mutation and self-approval isolation matrix |
| 12 | evidence-invalidation | candidate-mutation gate/review invalidation matrix |
| 13 | exhaustion-fails-closed | budget, retry, and timeout terminal/evidence matrix |
| 14 | daemon-lifecycle-recovery | restart campaign across every active lifecycle phase |
| 15 | provider-contracts | interruption, duplication, ordering, retry, malformed output, partial call, cancellation, and idempotency matrix |
| 16 | migration-and-export | all historical migrations and portable evidence export |
| 17 | evolution-isolation | sealed-input/evaluator/profile/policy/self-promotion red-team campaign |
| 18 | promotion-and-rollback | immutable promotion gate matrix and atomic history-preserving rollback |
| 19 | observability-and-redaction | source citations, failure classification, and seeded-secret redaction |
| 20 | load-and-soak | representative load SLO report and required eight-hour soak |
| 21 | public-surface-documentation | complete public reference inventory and command/protocol end-to-end matrix |
| 22 | architecture-integrity | dependency, ownership, generation, god-root, and API-leakage audit |
| 23 | representative-campaign | independent Rust, TypeScript, Python, Java, and mixed-repository campaign |
| 24 | release-artifacts | reproducibility, signatures, SBOM, provenance, license notices, migration/recovery docs, and completed security review |
| 25 | no-release-debt | quarantine/ignored-test, release-finding, unsafe-documentation, and placeholder audits |

`PRODUCTION_CRITERIA` contains exactly 25 definitions in this order. `REQUIRED_EVIDENCE` contains 44
closed requirements in stable order. Every requirement maps to exactly one criterion and exactly one
allowed source kind. All required artifacts must be independently reviewed and source-authenticated.

## H0-H3 qualification boundary

H4 does not depend on effectful H0-H3 crates. Instead, `QualificationObservation` models an admitted
signed report with:

- a closed slice identity (`H0Security`, `H1Resilience`, `H2Platform`, or `H3Performance`);
- the complete exact-current evidence binding;
- explicit `Ready` or `NotReadyForProduction` status;
- report and detached-signature digests;
- admitted signer identity; and
- independent-review status.

All four reports are mandatory. Missing, stale, mismatched, unreviewed, explicit-not-ready, or
conflicting reports fail closed. Multiple identical ready attestations are harmless; disagreement in
report digest or verdict is a conflict.

## Evidence and deterministic diagnostics

Raw evidence order is unrestricted. For each of the 44 requirements the evaluator scans the full
bounded input and computes one canonical `EvidenceAssessment`:

- contributing current count;
- stale, candidate/revision mismatch, wrong-source, unreviewed, and unsigned counts;
- disagreement between contributing artifact digests; and
- an order-independent aggregate of contributing digests.

A requirement is satisfied only when at least one exact-current reviewed signed observation exists
and every failure count is zero. Stale and mismatched artifact bytes are never mixed into the
contributing digest. Diagnostics are emitted in requirement ID order and a fixed reason order, then
H0-H3 order, review order, and finding/waiver order. No diagnostic cites a raw input position.

This yields identical verdicts, assessments, diagnostics, and decision fingerprints for every
permutation of the same observation multisets.

## Independent review, findings, and waivers

H4 requires at least two clean approvals. Each must be exact-current, producer-independent, use a
distinct reviewer identity, and use a distinct fresh-context digest. A self-review,
changes-required outcome, duplicate reviewer, shared context, stale/mismatched review, or conflicting
state for one review ID fails closed.

Finding state is also exact-current and identity-conserving:

- open findings block;
- unresolved findings marked release-blocking cannot be waived;
- ignored and quarantined findings always block;
- a non-release-blocking finding may be `WaiverRequested` only with a current approved waiver from
  an authority other than its reporter;
- waivers for resolved, absent, release-blocking, stale, mismatched, self-authored, or non-requested
  findings are invalid; and
- conflicting current states for one finding ID block.

The separate criterion-25 artifact audits remain required as evidence that the repository itself has
no ignored failure, quarantine, undocumented unsafe block, placeholder, or unresolved blocker. The
runtime finding reducer is an additional independent check, not a substitute.

## Verified claims

The executable `ReleaseDecision` has a private constructor. `is_ready` refines its specification
predicate, and `ready_implies_final_obligations` establishes that a ready result has:

- all 25 canonical criterion assessments satisfied;
- all 44 required artifact assessments satisfied;
- exact-ready H0, H1, H2, and H3 assessments;
- complete independent review;
- no blocker or invalid waiver; and
- no diagnostics.

Evidence contribution itself is specified by exact candidate/revision/time/source/review/signature
conditions. Dedicated proof obligations state that stale or mismatched bindings cannot contribute,
and that observation permutation preserves existence of contributing evidence. Runtime integration
tests additionally compare verdict, decision digest, every canonical assessment, and diagnostics
after reversing all input collections.

These claims do not prove cryptographic authenticity, Git object correctness, clock integrity,
runner behavior, platform behavior, or publication authorization. H4 consumes those facts only after
the owning integration admits authenticated observations.

## Integration contract

The release owner must:

1. authenticate evidence and signer registries before constructing H4 values;
2. build the exact candidate only after the release commit, target matrix, toolchain, profile,
   schemas, source revision, and manifest are frozen;
3. retain all raw artifacts named by their digests outside the policy crate;
4. evaluate at the release clock tick recorded in the retained decision artifact;
5. retain the complete decision, diagnostics, and canonical evidence inventory; and
6. obtain separate publication authority after `Ready` before tagging, signing, uploading,
   deploying, or changing any production pointer.

Workspace integration must register `peritus-release-policy` in the root workspace and architecture
policy, update the locked dependency graph, add it to formal verification/no-cheating inventories,
and include its tests and rustdoc compile-fail check in Gate A.
