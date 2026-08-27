# peritus-release-policy

`peritus-release-policy` is the V-class H4 policy core that reduces retained release evidence to
one deterministic `Ready` or `NotReadyForProduction` decision for an exact release candidate.

The candidate identity binds a nominal candidate ID, exact Git commit, complete semantic version,
Linux/macOS/Windows target profiles, Rust/Verus/vstd/solver closure, runtime qualification profile,
policy/evidence/report/artifact schemas, producing revision, and release-manifest digest. Every
artifact, qualification, review, finding, and waiver observation separately binds that candidate,
a monotonic validity window, positive sequence, and producing source revision.

The evaluator checks, in order:

1. all 44 artifact requirements mapped to the architecture's stable 25 production criteria;
2. signed, reviewed, exact-current H0 security, H1 resilience, H2 platform, and H3 performance
   qualification reports;
3. two distinct fresh-context independent approvals with no self-review or changes-required result;
4. current finding and waiver state, rejecting unresolved blockers, open findings, quarantine,
   ignored findings, self-waiver, stale waiver state, and conflicting finding identities; and
5. an empty canonical diagnostic set.

Input ordering never provides semantics. Artifact requirements, criteria, and qualifications are
projected into stable catalog order; review and finding failures are order-independent aggregate
facts. The decision fingerprint mixes only the exact candidate manifest and canonical current
assessments. Stale, mismatched, wrong-source, unreviewed, or unsigned observations cannot satisfy a
requirement or supply its contributing digest.

## Boundary

`Ready` is a policy fact, not authority. This crate has no API for tagging, signing, publishing,
uploading, deploying, mutating a production pointer, or issuing a capability. Integration must
authenticate inputs before construction and separately authorize any publication step after
retaining the complete decision artifact.

H0-H3 crates are deliberately not dependencies. Their signed qualification observations cross the
H4 boundary as inert typed policy inputs, which keeps the V-class decision independent of their
effectful implementations.

## Example

Applications normally collect the five observation vectors through authenticated evidence stores,
then call:

```rust,ignore
let evidence = ReleaseEvidence::new(
    artifacts,
    qualifications,
    independent_reviews,
    findings,
    waivers,
)?;
let decision = evaluate_release(candidate, monotonic_release_tick, &evidence);
if !decision.is_ready() {
    retain_diagnostics(decision.diagnostics());
}
```

See `docs/h4-release-policy.md` for the evidence map, diagnostics, formal claims, and integration
contract.
