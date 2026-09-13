# Independent exact-source review: release336 to release377

Date: 2026-09-12

## Verdict

PASS for the bounded release377 production-output correspondence increment. I found no source-review blocker in the corrected 52-file to 64-file patch. The actual `evaluate_release` traversal is connected to an input-defined readiness predicate, its exact stored assessments, the canonical diagnostic sequence, and the current decision-fingerprint byte reducer without executable preconditions, assumptions, admitted bodies, or verification-only stand-ins.

This verdict is limited to the exact source identities below. It is not approval of the withdrawn reconstructed preimage, publication authority, external observation truth, cryptographic authenticity, or the remaining constructor proof gaps.

## Exact source identities

- Prior root: `/tmp/peritus-parent-release336-source-exact/crates/foundation/peritus-release-policy`
- Prior manifest: `/tmp/peritus-parent-release336-source-exact.sha256`
- Prior manifest SHA-256: `a1369b4a9b011130293b7389b7a09d6cd8a7726490597c4c0f1cc22fb1c50483`
- Prior files checked: 52 of 52
- Final root: `/tmp/peritus-parent-release377-source-exact/crates/foundation/peritus-release-policy`
- Final manifest: `/tmp/peritus-parent-release377-source-exact.sha256`
- Final manifest SHA-256: `14db526a3582c733e530ed94b4dab892cfeb125fb956203d17bf662f34c68a82`
- Final files checked: 64 of 64
- Exact patch: `/tmp/peritus-parent-release336-to-377-exact.patch`
- Patch SHA-256: `e5cbd1eea25cb0246f952af2c31ab647f0e8e9696d52d127232a3452ba0ebc4b`
- Patch size: 5,607 lines; 43 changed paths
- Changed-path description: `/tmp/peritus-parent-release336-to-377-exact.json`
- Changed-path JSON SHA-256: `61ba096f397477d0e739bed6575c4d390cd7aa9b5b5fc1bdfa94af49dfe4e191`

I did not use the withdrawn 44-file reconstructed release336 preimage or its diff.

## Source findings

### Production readiness and exact outputs

- `release_inputs_ready` is a conjunction over the actual supplied `ReleaseEvidence`, exact `ReleaseCandidate`, and `evaluated_at`. It covers every artifact requirement, every derived criterion, all four qualification slices, the complete review reduction, and the complete finding/waiver/request reduction.
- `evaluate_release` has no executable precondition. It calls the verified production reducers, emits diagnostics, and passes their actual results to the private `ReleaseDecision::from_evaluation` constructor. Its postcondition ties the returned raw verdict, public `is_ready`, and diagnostic emptiness iff to the input predicate.
- The separate public `ReleaseQualificationAdmission::evaluate` remains an exact conjunction of the verified decision's `is_ready` value and all eight fixed `ReleaseQualificationCheck` slots. It does not manufacture readiness or publication authority.
- The artifact characterization is genuinely input-defined: for each requirement, at least one contributing observation exists; every observation naming that requirement must have the required source, exact current binding, review, and signature state; and all contributors must match the first contributing artifact digest. The executable loop has the same branch priority: mismatch, stale, wrong source, review/signature flags, contribution, digest conflict.
- The qualification characterization is input-defined: every matching-slice report must be exact-current, reviewed, and `Ready`; at least one ready report must exist; and every reduced report must agree with the first report digest and verdict. This matches the actual mismatch, stale, unreviewed, verdict-count, report-conflict, verdict-conflict, and XOR paths.
- Review readiness requires every supplied review to be current, approved, non-self, and independently attested; a two-review quorum; and all current pairs to have distinct reviewer identities and context digests and to agree when a review ID repeats. The quadratic executable pair scan and its flags match that relation.
- Finding readiness covers every finding, finding pair, waiver, waiver pair, and nonblocking waiver request. The executable three-pass reducer matches the model's mismatch/staleness priority, blocker/open/ignored/quarantined counts, eligible-waiver lookup, conflicting-finding flag, invalid-waiver base and pair counts, and missing-request-waiver count.

### Closed 44/25 slot correspondence

A mechanical source comparison found:

- 44 `EvidenceRequirement` enum variants
- 44 executable `assess_evidence` slots
- 44 `assessments_match_inputs` requirement slots
- 44 `all_requirements_satisfied` conjuncts
- all four requirement sequences identical
- 25 `AcceptanceCriterion` enum variants
- 25 executable `assess_criterion` slots
- 25 criterion assessment-spec slots
- all three criterion sequences identical

The 25 criterion values use the same fixed evidence-index conjunctions in the executable and spec functions. `criteria_cover_all_requirements` proves their full conjunction iff the complete 44-requirement conjunction, so criteria do not introduce a circular readiness boolean.

### Counts, conflicts, and diagnostics

- Every modeled `u16` counter uses the same `65535` saturation rule as the executable `saturating_add(1)`. This includes invalid-waiver pair counts, which can exceed one collection's 4,096-entry bound.
- Artifact and qualification aggregate digests use the exact executable XOR byte relation. First-contributor/report state and disagreement flags retain the actual fold order while the accepted equality condition is order independent.
- `canonical_diagnostics` exactly concatenates evidence diagnostics in 44-slot order, qualification diagnostics in H0-H3 order, review diagnostics, then finding diagnostics. Within each assessment, the spec variant, payload, and order match the executable push order. The append contracts also prove no-growth iff the corresponding assessment is diagnostically clear.
- The top-level contract stores the exact canonical diagnostic sequence and proves its emptiness iff the input readiness predicate.

### Decision construction and fingerprint

- `ReleaseDecision::from_evaluation` is crate-private and retains the exact candidate, evaluation tick, 25 criteria, 44 evidence assessments, four qualifications, review assessment, finding assessment, diagnostics, raw verdict, and fingerprint.
- The fingerprint proof models the current implementation byte for byte: it starts from all 32 manifest bytes; applies each evidence slot's selected digest byte, low contributing-count byte, and satisfied bit; applies each qualification slot's selected report byte and satisfied bit; then applies verdict, review, and finding summary bytes.
- The fingerprint is not injective or cryptographically authenticated. It intentionally samples one byte from each contributing/report digest, uses XOR and low count bytes, omits criteria as separately encoded fields, omits `evaluated_at`, and relies on the caller-supplied candidate manifest digest for candidate binding. The proof establishes deterministic correspondence to this reducer only.

### Constructors, cfg, API, and Clone

- Exact success conditions, retained fields, and represented error kind are present for `EvidenceBinding`, `EvidenceObservation`, `ReleaseEvidence`, `QualificationObservation`, `ReviewObservation`, `FindingObservation`, and `WaiverObservation`.
- `EvidenceBinding::construction_error` proves the only multi-kind priority added here: zero sequence/source revision precedes an inverted interval. Other strengthened observation constructors have one externally distinguishable error kind, so their two-digest internal order is not observable through `ConstructionErrorKind`.
- Added `cfg(verus_only)` items are models, declarative proof modules, or proof-only imports/exports. I found no executable branch hidden behind a new verification cfg.
- The patch adds no broad lint suppression, no `assume`, `admit`, axiom, external body, `assume_specification`, or verifier escape. The new Clippy allowances are narrow, reasoned argument/line-size annotations on the explicit reducers.
- All public assessment, diagnostic, binding, observation, candidate component, identity, and admission values that derive `Clone` also derive `Copy`. `ReleaseEvidence` and `ReleaseDecision`, which own `Vec` fields, do not derive `Clone`. I found no new non-Copy autoderived Clone proof gap.
- Public runtime exports are preserved. The final source makes only `validation` crate-public; the readiness/model modules remain private Rust modules.

## Remaining feasible proof gaps

These do not invalidate the bounded evaluator-output correspondence, but they remain real work:

1. Candidate-family checked construction is not yet completely specified. `GitCommitId::{sha1, sha256}`, `PlatformIdentity::new`, `PlatformMatrix::new`, `ReleaseVersion::new`, `ToolchainIdentity::new`, `ProfileIdentity::new`, `SchemaIdentity::new`, and `ReleaseCandidate::new` lack exact public Result/field/error-priority postconditions. `require_revision` also lacks its straightforward exact postcondition. The outer nominal ID constructors lack explicit Result/error postconditions even though the private `StableIdentity::new` contract and type invariant enforce nonzero bytes.
2. `ReleaseCandidate::new` checks only positive source revision and nonzero manifest digest. It does not prove or execute canonical serialization/hash binding between the manifest digest and the other candidate fields. Evidence matching remains all-field/all-byte exact because `candidate_matches_exactly` compares every field plus the manifest; the manifest's claimed content provenance is still supplied externally.
3. The deterministic decision fingerprint has no collision-resistance, authenticity, or signature theorem, and its encoding is not a canonical serialization of every returned decision field. A separate authenticated decision artifact is still required by its own documentation.
4. The named ghost predicates `release_inputs_ready` and `ready_evaluation_contract` live in the private `evaluator` module and are not re-exported. The public function carries the proved postcondition internally, but downstream proof clients cannot directly name/reveal these predicates through the crate root. This is a proof-API composability gap, not a runtime or soundness defect in this checkpoint.
5. External truth and authority remain outside this pure reducer: actual command execution, repository and platform facts, signer authentication, digest provenance, filesystem/network/provider I/O, publication/finalization authority, and deployment.
6. The bounded pair scans for review/finding/waiver conflict detection are proved functionally but have no verified complexity bound. This review did not benchmark their worst-case 4,096-entry inputs.

## Evidence inspected and commands run

Read-only identity checks executed by this reviewer:

```text
sha256sum /tmp/peritus-parent-release336-source-exact.sha256 \
  /tmp/peritus-parent-release377-source-exact.sha256 \
  /tmp/peritus-parent-release336-to-377-exact.patch \
  /tmp/peritus-parent-release336-to-377-exact.json

(cd /tmp/peritus-parent-release336-source-exact/crates/foundation/peritus-release-policy \
  && sha256sum -c /tmp/peritus-parent-release336-source-exact.sha256)

(cd /tmp/peritus-parent-release377-source-exact/crates/foundation/peritus-release-policy \
  && sha256sum -c /tmp/peritus-parent-release377-source-exact.sha256)
```

Both manifests reported every listed file `OK`. I also ran read-only source scans for public executable `requires`, trusted constructs/escapes, cfg additions, lint allowances, derived Clone shapes, and a Python source-order comparison for the 44 requirement and 25 criterion sequences.

I did not rerun Cargo or Verus for this independent source review. I inspected, but did not produce, the following independent parent qualification:

- `/tmp/peritus-parent-release377-verus-independent.log`, SHA-256 `997eaf8bcc2b60c6485a8ad1236a20912b787ba3c8f40a3fe75864a9eded4ac0`: pinned strict command with `--no-cheating --rlimit 20`; dependencies 2,044/0 and 210/0, release policy 377 verified/0 errors. One auto-trigger note appears in preexisting `src/model.rs`; it is not a verification error.
- `/tmp/peritus-parent-release377-tests-independent.log`, SHA-256 `021fa5cf80b5c54b81cfc506425a6138826580c6cf22fffcaed8a1eff975edec`: 28 executable release-policy tests plus one compile-fail doc test, all passed.
- `/tmp/peritus-parent-release377-qualification-independent.json`: records those exact commands and exit code 0.

I also inspected the worker evidence manifest `/tmp/peritus-sol-release-output-evidence-20260912T001.sha256` (SHA-256 `9760738b5b013cf21801969ad617e53ba3743bff376da202c7ba6237ee5ee4d7`) and its referenced final logs. Those supplied logs report strict Clippy, fmt, ordinary API scan (`3,492` formal-boundary files and `14,680` ordinary-safe executable entry points), and source-layout scan (`4,393` files) passing. Those checks were produced by the worker, not rerun by this reviewer.

No source, manifest, CI, ledger, branch, or repository setting was changed by this review.
