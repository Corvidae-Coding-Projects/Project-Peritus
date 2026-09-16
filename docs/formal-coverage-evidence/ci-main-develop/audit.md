# Independent review of main/develop checker custody

The parent reviewed the nine exact files in `source-sha256.txt`, including the complete workflow,
its compiled validation policy and tests, the complete delta from the earlier custody checkpoint,
and the checker manifest and compiler dependency inventory. The reviewed change supports both
`main` and `develop` while separating the workflow/checker commit, PR comparison base and candidate.

The event context is checked against the exact workflow revision and default branch. This agrees
with GitHub's current description of the default-branch context for
[`pull_request_target`](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request_target),
rechecked during this review. The base and candidate remain separate immutable checkouts; proof
impact uses the PR base. A main-targeting PR additionally requires base and checker identity.

Both checker-to-base and base-to-candidate comparisons preserve the complete protected checker
inputs before Rust installation, Cargo or metadata. All three trees reject nonregular entries and
alternate root Cargo/toolchain selectors. The checker source, manifest, lock, configuration and
embedded workflow/ruleset inputs are protected. Current compiler dependency paths outside
`xtask/src` are exactly the two protected workflows and the protected ruleset template. The current
checker has no repository path dependency or default build script. Candidate evaluation invokes
the checker built from the exact workflow revision, with offline metadata and restricted executable
lookup, then evaluates trust against the exact PR base.

## Independently reproduced evidence

Ten focused policy tests passed. The canonical reproducibility check passed with 141 immutable
action references. In addition, `guard-smoke.py` executed the unchanged identity and pre-metadata
guard scripts from the frozen workflow in disposable local Git repositories. All twelve cases
behaved as expected: unchanged main and develop histories passed; incorrect event/checker/base/head
identities, unsupported branch, wrong repository ID, main/checker mismatch, checker-input changes
on either comparison edge and an alternate Cargo selector failed at the expected guard.

The guard harness requires Python and PyYAML. It takes a workflow path as its only argument. Its
retained JSON binds the result to the workflow SHA-256. It executes neither Rust nor Cargo metadata
nor candidate programs. It does not simulate GitHub's event producer or an external approval.

Implementation Clippy, formatting, actionlint and whitespace checks passed. The implementation's
full package attempt passed 427 unit tests with one ignored test, then failed one CLI integration
test on a concurrently edited scheduler file's line budget. That failure is retained; it is not
reported as a full package pass. Final package, platform and hosted runner checks remain required.

## Bounded verdict and remaining deployment

PASS for this repository implementation and its two protected-input comparisons. This closes the
earlier checkpoint's main-only validation limitation. It does not establish exclusive hosted merge
authority. No App producer, credential, live ruleset change or external bootstrap was installed by
this increment. A legitimate checker/dependency transition still needs the separately controlled,
reviewed bootstrap documented in `docs/github-governance.md`; identical protected inputs must then
exist on main and develop. The local positive fixture only tests an already installed baseline.

The workflow's ordinary Actions result is not an exclusive producer identity. Protection against
candidate-controlled status production, final-head enforcement and final proof-impact records
remain required work. This checkpoint changes no mandatory GitHub review count and preserves the
maintainer's self-merge policy. It is not an obligation discharge or a final runner qualification.
