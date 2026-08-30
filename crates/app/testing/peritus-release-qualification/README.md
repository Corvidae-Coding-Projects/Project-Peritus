# peritus-release-qualification

`peritus-release-qualification` is the H4 effect shell. It collects required campaigns on distinct
fresh subjects, admits only detached-signature-verified evidence envelopes bound to one exact
candidate, requires all 25 production acceptance criteria to map to evidence, validates an
independent final audit and finding closure, constructs a content-addressed evidence manifest, and
reduces the result through a narrow deterministic release-policy adapter.

The crate fails closed. Missing reports, missing native platforms, incomplete cleanup, a nonmatching
binding, nonreproducible artifacts, an incomplete manifest, an open audit finding, an unavailable
policy, or a policy rejection produces `NotReady`. The crate does not sign, tag, publish, mutate
the candidate, invoke Git, or treat construction-time test data as executed release evidence.

The `VerifiedReleasePolicyAdapter` links the authenticated H4 input to the separately verified
`peritus-release-policy` evidence aggregate, rejects binding or digest drift, and translates only
the policy's deterministic terminal decision into `PolicyDecision`. It does not duplicate release
rules or manufacture policy evidence.

The `peritus-h4` binary is the operator boundary. `envelope` writes the candidate-bound bytes an
external Ed25519 signer must authenticate. `verify` checks returned public material and retains a
canonical admitted record. `finalize` reads the strict qualification plan and external evidence
root, re-verifies all 25 required signed inputs plus the independent final audit, reconstructs the
11 fresh-subject cleanup records, compares independent builds, assembles all 25 acceptance
criteria, evaluates the verified release policy, and writes one no-overwrite final bundle. It exits
successfully only for `Ready`.

The plan schema and deliberately non-passing operator template live at
`release/schemas/h4-qualification-plan-v1.schema.json` and
`release/templates/release-inputs.template.json`. The complete operating sequence is documented in
`docs/h4-release-qualification.md`.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-release-qualification
CARGO_BUILD_JOBS=2 cargo clippy --locked --package peritus-release-qualification \
  --all-targets --all-features -- -D warnings
```
