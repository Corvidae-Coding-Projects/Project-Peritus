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

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-release-qualification
```
