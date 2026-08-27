# H4 release qualification

H4 is the final release-evidence boundary. It does not make an implementation complete, replace
H0–H3, sign artifacts, create a tag, publish a package, or infer a favorable result from missing
data. It binds observed evidence to one exact candidate and asks the separately owned deterministic
release policy for the final decision only after all H4 structural checks pass.

## Exact candidate identity

Every artifact, report, campaign, comparison, manifest, and audit binds the same `ReleaseBinding`:

- a full lowercase 40- or 64-hex Git object identity;
- canonical semantic release version without a leading `v`;
- exact Rust/Verus toolchain identity;
- target triple plus native platform revision; and
- independently computed SHA-256 of the source tree.

An abbreviated commit, a version label without the candidate tree, or a cross-compiled artifact
without its native platform identity cannot substitute for this tuple. Candidate mutation requires
a new binding and invalidates every prior signature envelope.

## Artifact and supply-chain contracts

`peritus-release-artifacts` is an effect-boundary library. Release adapters give it bytes and
external observations; it does not inspect ambient Git state, read a clock, invoke a build, obtain
credentials, or perform publication.

`ArtifactInventory` sorts entries by normalized release-relative path and records exact length,
SHA-256, media type, and one or more closed roles. Empty inventories, duplicate paths, escaping
paths, and unbounded collections are rejected. Its compact JSON is canonical input to content
addressing.

`SpdxDocument` emits deterministic SPDX 2.3 JSON from an explicit component list. Components carry
stable SPDX identifiers, exact versions, suppliers, download locations, concluded and declared
license expressions, and SHA-256 checksums. The caller supplies the creation timestamp; generation
never reads ambient time. Component order and `DESCRIBES` relationships are canonical.

`ProvenanceStatement` emits an in-toto Statement v1 with the SLSA provenance v1 predicate. Subjects
come directly from the complete artifact inventory. The predicate binds the release tuple, builder,
invocation, build type, start/finish observations, and a URI-sorted material list. A provenance
statement cannot be constructed against an inventory for another candidate.

`verify_detached_ed25519` accepts only a public key, a detached signature, and exact payload bytes.
It exposes no key generation or signing function and retains payload and signature digests after
successful verification. H4 signed evidence uses a domain-separated canonical envelope containing
the complete release binding, evidence kind, retained path, byte length, and payload digest. This
prevents a valid report signature from being replayed for another candidate or evidence role.
The envelope also binds `Satisfied` or `NotSatisfied`; every required input must be signed as
`Satisfied`, so an authentic failing H0–H3 report or campaign remains a release blocker.

`compare_builds` requires distinct builder identities and the same release binding. It compares the
complete path-sorted output, including lengths and hashes, and reports missing or changed artifacts.
A comparison containing any difference is evidence of nonreproducibility, never a warning that can
be silently ignored.

The documentation inventory requires exactly one migration, backup, restore, rollback, license
notice, and completed security-review document. License notices are rendered from an explicit,
component-sorted input set. Empty documents and incomplete category sets are rejected.

## Fresh-subject collection

`peritus-release-qualification` owns orchestration contracts, not the platform effects. A
`FreshSubjectFactory` provisions a new disposable subject for each closed campaign, and the
`QualificationSubject` adapter collects an externally signed evidence envelope and then consumes
itself during cleanup. Subject identities cannot repeat. Cleanup reports remaining processes,
mounts, worktrees, and temporary paths; any nonzero count or unobserved cleanup fails the case.

The v1 catalog runs in this order:

1. Gate A;
2. locked Foundation verification/build;
3. native Linux matrix;
4. native macOS matrix;
5. native Windows matrix;
6. long-duration soak;
7. representative Rust campaign;
8. representative TypeScript campaign;
9. representative Python campaign;
10. representative Java campaign; and
11. representative mixed-repository campaign.

A provisioning or collection error remains a failed case. The runner still attempts cleanup after
collection failure. Cross-compilation, subject reuse, substituted evidence kind, another candidate
binding, or incomplete cleanup cannot become a passing observation.

## Required signed inputs

Before final policy evaluation, H4 requires exactly one detached-signature-verified input for:

- H0 security;
- H1 resilience;
- H2 Linux, macOS, and Windows native qualification;
- H3 performance qualification;
- every fresh-subject campaign above;
- artifact inventory, SPDX SBOM, provenance, artifact signatures, and independent-builder
  reproducibility;
- migration/recovery documentation and license notices;
- the complete AC-01 through AC-25 evidence map; and
- the independent final audit.

The structured artifact inventory and reproducibility comparison are also supplied directly. H4
recomputes their canonical digests and requires equality with the corresponding signed payload.

## Acceptance-criterion traceability

`AcceptanceCriterion` is the exact 25-item catalog in
`.design/peritus-production-architecture.md`; the stable identifiers are `AC-01` through `AC-25`.
`CriterionEvidenceMap` requires every item exactly once and at least one authenticated evidence
reference per item. References must also occur among the evidence admitted for this exact H4 run.
The authoritative meanings are:

| ID | Required demonstration |
| --- | --- |
| AC-01 | Clean ordinary Rust and end-to-end suites on every tier-one platform. |
| AC-02 | Clean locked workspace Verus verification and production build without unapproved trust. |
| AC-03 | Complete proof-obligation inventory or approved narrow exclusions with compensation. |
| AC-04 | Trusted-construct allowlist, threat analyses, and refinement tests. |
| AC-05 | Privileged values only through verified transitions. |
| AC-06 | Every illegal lifecycle edge rejected consistently across proof and tests. |
| AC-07 | Crash/power-loss recovery at every durable commit boundary. |
| AC-08 | Byte-identical replay from empty projections. |
| AC-09 | Full malicious-repository and secret-exfiltration suite. |
| AC-10 | Native sandbox conformance and independent escape reviews. |
| AC-11 | Writer/reviewer/fixer authority isolation. |
| AC-12 | Candidate mutation invalidates stale evidence. |
| AC-13 | Exhaustion is non-success and retains complete evidence. |
| AC-14 | Restart reconciliation across every active lifecycle phase. |
| AC-15 | Provider interruption, malformed output, cancellation, and retry contracts. |
| AC-16 | Historical migrations, corruption rejection, and portable export. |
| AC-17 | Evolution red-team protection of sealed evaluation and authority. |
| AC-18 | Fully gated promotion and atomic, history-preserving rollback. |
| AC-19 | Causal observability, failure taxonomy, citations, and redaction. |
| AC-20 | Load and soak objectives for latency, capacity, memory, cancellation, and recovery. |
| AC-21 | Public command/protocol documentation, examples, errors, and end-to-end tests. |
| AC-22 | Dependency, architecture, generated-file, and API-boundary checks. |
| AC-23 | Independent representative Rust, TypeScript, Python, Java, and mixed campaigns. |
| AC-24 | Reproducible signed artifacts, SBOM/provenance, notices, recovery docs, and security review. |
| AC-25 | No quarantines, ignored failures, release blockers, undocumented unsafe, or placeholders. |

The checked `release/templates/acceptance-evidence-map.template.json` is deliberately incomplete.
Empty evidence lists cannot construct a `CriterionMapping` and are not qualification evidence.

## Independent audit and manifest ordering

The final audit binds the candidate, auditor identity, canonical release-contributor set,
pre-audit evidence-set digest, and a canonical finding list. Auditor independence is checked
against that signed contributor set before the audit is admitted. High and critical findings must be `Closed` with signed closure
evidence; risk acceptance is not closure for a release blocker.

Self-reference is avoided explicitly:

1. construct the content-addressed manifest entries for every item except the final audit;
2. compute `EvidenceManifest::pre_audit_digest`;
3. have the independent auditor review that exact evidence set and sign the audit envelope;
4. add the final-audit entry; and
5. compute the final manifest digest.

The final manifest requires one role for every signed H4 input, rejects duplicate roles and paths,
and retains payload and signature-envelope digests. The final report binds the artifact inventory,
criterion map, final audit, and final manifest digests.

## Fail-closed reduction

`QualificationReport::evaluate` does not call release policy until all structural checks pass.
Missing input, duplicate evidence, binding mismatch, failed fresh subject, incomplete cleanup,
digest disagreement, nonreproducibility, unreferenced criterion evidence, incomplete manifest,
nonindependent audit, pre-audit digest mismatch, or open blocking finding yields `NotReady` and a
typed blocker list. Policy rejection or unavailability also yields `NotReady`. Only a complete H4
input plus `PolicyDecision::Ready` can derive the `Ready` enum variant; callers cannot set the report
verdict directly.

## Composition and operating boundary

`VerifiedReleasePolicyAdapter` is the composition-owned bridge to `peritus-release-policy`. It
requires exact release-binding, artifact-inventory, evidence-manifest, criterion-map, and final-audit
digests; verifies that the C-class binding corresponds to the V-class candidate manifest, commit,
and version; links every supplied verified-policy artifact observation to signature-bound H4
criterion evidence; and links H0, H1, H3, and the canonical aggregate of all three H2 platform
reports before delegating to `evaluate_release`. Any drift returns `PolicyDecision::Unavailable`.
The adapter neither duplicates release rules nor converts policy failure into readiness.

The actual release operator must instantiate native fresh-subject adapters and retain the observed
bytes outside the source tree under a candidate-specific evidence root. Those adapters must supply
the cryptographically verified observations used to construct both the H4 input and the verified
policy evidence aggregate. Test fixtures and repository templates are not execution evidence.
Until the real candidate-bound campaigns, signatures, independent reviews, and final audit have
been collected, the truthful final state remains not ready.
