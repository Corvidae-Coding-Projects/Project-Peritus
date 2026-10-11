# Independent review: run-knowledge constructor admission

## Verdict and reviewed identity

Bounded PASS. I found no correctness or claim-boundary blocker in the frozen 18-file constructor-admission increment. I reviewed the frozen full package at `/tmp/peritus-parent-knowledge-admission-source`, not the later planner-error follow-up. I made no source changes and did not integrate the increment.

I independently ran both frozen manifests with `sha256sum -c`; all 18 changed paths and all 37 package paths matched. The manifest identities are:

- changed 18: `fc38322696054b46f777d95c1a457c3a8177d59728b2efae3e3c94f281cdea1a`
- full package 37: `67b8b133b4fc0f0c5670333d6c05b7fa1736df1398ef763731d6ad7a78e57bb8`
- preimage-to-increment patch: `10be43ce8af89af3cc048d42efdbdffe59ba79231b5fb2ea4c6edda766a6a656`
- supplied qualification record: `22802cc9f3826415e95d7473947aaab671db67232aa4cd1ad0e684f4c4bd2ea4`
- parent claim/limit audit: `2f9f58257e1dc1852a116de05a7af6f9fa2a1adf39a6de5f0b50affcbf3ac299`

## Source and caller review

The public constructors now expose `Ok` iff contracts over the actual supplied values, retain all successful fields, and characterize existing failure order and payload fields:

- `KnowledgeLimits::new` accepts exactly four nonzero limits and retains each limit.
- Both 16-byte identity constructors accept exactly nonzero identities and retain every byte. The new canonical comparison recursively covers all 16 bytes and agrees with the runtime ordering used by source, section, and target collection validation.
- `KnowledgeError` specs, private constructors, and public getters agree on the category plus exact optional section/source/numeric fields. Each specialized constructor clears every unrelated detail.
- Source validation applies required-emptiness, maximum, then first adjacent ordering failure. Duplicate and descending errors retain the rejected current source identity. `KnowledgeBinding::new` keeps role, sequence, then source-validation precedence; `CurrentKnowledgeState::new` uses the same reducer with its catalog limit.
- The shared section-ID validator checks the optional excluded identity before the adjacent ordering failure at each position. `KnowledgeSection::new` checks dependency count first, then uses that validator with its own ID excluded. `InvalidationRequest::new` checks change shape first and uses the same validator without an exclusion.
- Snapshot validation keeps role, minimum section count, maximum section count, then per-section identity order, lineage, role, creation time, and first missing/forward dependency. Required typed references are checked after every section. The runtime helpers and spec predicates agree on that order and on the exact rejected identity.

The strengthened type invariants are established by the only public constructors because the fields remain private. The reviewed `Clone` implementations retain the complete semantic fields needed by those invariants; snapshot cloning additionally transports canonical identity, unique backward topology, typed references, binding lineage, role, and time. Existing production consumers in invalidation and delta planning use the same candidate, source, section, request, and snapshot views, so the stronger contracts describe their actual inputs rather than a verification-only stand-in.

The snapshot rule deliberately uses `CandidateIdentity::same_lineage`: equal run and workspace are required, while candidate digest and conversation revision may differ. Section creation is compared with the supplied snapshot checkpoint. Later invalidation planning still checks conversation/candidate freshness. Snapshot construction applies only the snapshot's section-count limit because nested bindings and sections were admitted by their own constructors; it does not reapply the snapshot's source/dependency limits. These are exact existing semantics and are covered by focused tests.

The exact collection predicates also support inputs that are not yet admitted, which is necessary for `Err` correspondence. Once values enter `KnowledgeSection`, `KnowledgeBinding`, `CurrentKnowledgeState`, `InvalidationRequest`, or `RunKnowledgeSnapshot`, their private-field type invariants retain only the intrinsic shape documented in source. I found no hidden public executable precondition, verification-only runtime branch, `assume`, `admit`, axiom, external body, or new lint suppression in the increment. The existing ghost-enum missing-doc allowance in `plan.rs` is outside these 18 changed paths.

## Evidence status

I inspected the supplied command record and result tails. I did not independently rerun the compiler or tests for this review. The supplied frozen evidence reports:

- strict pinned Verus, `--no-cheating --rlimit 20`: 216 verified, 0 errors;
- default ordinary suite: 29 passed;
- all-feature ordinary suite: 29 passed;
- all-target/all-feature Clippy with `-D warnings`: pass;
- package formatting: pass;
- isolated ordinary API scan: 3387 files / 14618 entry points, pass;
- isolated source-layout scan: 4273 files, pass.

I also inspected the supplied negative metadata and log. Mutating only the actual descending-source error to report the previous identity produced 215 verified / 1 error; the recorded restored source produced 216/0. This supports sensitivity of the exact error-identity contract. It does not substitute for external authenticity of the recorded artifacts.

## Bounded claims and open work

This increment proves deterministic admission, field retention, intrinsic invariants, and constructor error precedence for the supplied in-memory values. It does not prove that source digests, candidate identities, filesystem observations, or other inputs are authentic. It does not prove persistence, rendering, external execution, or end-to-end resume behavior.

It also does not cover the later public planner-error payload increment. Exact planner error payloads, broader context selection/rendering, and the remaining end-to-end obligation discharge stay open. Snapshot admission intentionally does not enforce nested limits again and intentionally permits different candidate digest/conversation within one run/workspace lineage. Those are reviewed policy boundaries, not omitted constructor checks.

## Changed-file hash correspondence

```text
5cb4990d4fe02067820e6211b7e50ff3576cb64d825ec4ce60be5ed179cf7201  src/binding.rs
339dc09031f7dea2a9d260c94b221748fd101a0bdd57de5ea0ddc5990d2f34a8  src/change.rs
7484f68480837c10e0add8bb017d505d2c2725bd21ae26c42b865f5214d8d68a  src/error.rs
e2ab602646ca110bed8eeef49d438be305fc2e78358946d8a19745576a3c3676  src/identity/collection.rs
f2cd8381b7feea6e1473e8411dc989523da52597771514f40f99ebcc662c0a4d  src/identity.rs
f5f7e650db6f1521bc478733df874fb4add2061b629b025bd5e149c4460a787e  src/limits.rs
220809f8f29e8ce85ad71676a858a42b81cd36d434471c249c172871c572ebbf  src/model/admission/sections.rs
27ca41d5e3095d7e25d9a7ef052a8df3abfae7e7c9aa8babaf52c2100e7a9b45  src/model/admission/snapshot.rs
a0ab8e9f2fa5e9c91e1630f39e8a5af97cad3b0e827906c034f719d79e56f539  src/model/admission.rs
2d094076b34607a85f6e24c5f45275bb1edd73bb77f99a28720502b583a3b8e8  src/model/topology.rs
00235c6a8dd0b7d7cee45f64799e3244a28bb44ca878a2f123ce743bc3de54f8  src/model.rs
8cd7de0eb15709f21d46113721a6a827b9df5dea72d67cdcfcac51eb514d8138  src/section.rs
3f73e4d55f849a5ed0b58db3750e156a26b74fe079bcac61f8eb9fa5460b6a6b  src/snapshot/validation.rs
51168542fcd1b501f603ae9d20c6fc9b7a999b09497f0598b49bbaf194906918  src/snapshot.rs
c4118c1d7d828ba0d2660ef9ab21e8e5bd3f34a90e18789745f5986661a5a1fb  src/source/validation.rs
67934ded380b55a08db89c6da6b133b8cd7108f6d4cf8bc574f472d0e6d3b0c5  src/source.rs
053b5ec43db3035be9102748a0b8dbd056deb5be3f60f3d629f3d4ae42a1a360  tests/admission.rs
5d66b14c18ab4be353a6f11136ec9f13ddb00a882f2c2aebc30e95e53d338874  tests/snapshot_admission.rs
```
