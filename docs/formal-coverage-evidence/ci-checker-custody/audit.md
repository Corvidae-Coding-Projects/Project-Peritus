# Trusted checker transition custody

Independent review of the earlier authority workflow identified a cross-revision gap: its
trusted checker could validate a PR that changed checker source, then that changed checker would
become the trusted base for a later PR. This was a static defensive finding, not a live attack.

## Reviewed correction

Before candidate Cargo metadata, the workflow now requires exact base/head equality for the
root Cargo manifest and lockfile, xtask manifest/source/possible default build script, root Cargo
configuration, line-ending attributes, Rust selector, both embedded workflow files and the embedded
ruleset template. The release dep-info inventory was inspected: all 131 reported local build inputs
are xtask source or those three embedded files. Root workspace metadata and locked dependencies
are protected separately by the manifest/lockfile guard. The full source tree guard also protects
future source additions and default build-script creation. Legitimate checker or dependency
transitions require a separately controlled exact-head bootstrap decision.

Parent reviewed all nine frozen files, the full workflow and validator, integration into the
all/reproducibility call path, the metadata-only child process surface and all focused fixtures.
The base-built validator still compares the authority workflow byte for byte and requires exact
YAML structure, immutable actions, event/checkout identities, ordered checks, read-only permissions,
offline metadata and failure propagation. No candidate action, script, package build or procedural
macro is intentionally executed by these validation steps.

## Evidence

Seven focused tests passed for both implementer and parent, including a disposable Git fixture
that rejects checker-source-only drift while accepting unrelated documentation changes. Parent
also reproduced the 140-reference reproducibility pass. Implementer retained 422 passing library
tests with one existing ignored test, strict Clippy, formatting, source layout, docs, release build,
actionlint and whitespace checks. The full package attempt failed later in CLI integration because
concurrent scheduler source used the then-rejected ghost cfg_attr; its failure is retained and is
not a package pass. The separately reviewed API correction subsequently passes its targeted tests
and actual source scan. Final whole-package verification remains required at stable source.

The complete checker-input fingerprint is the implementer's historical freeze. Parent's later
API-scanner change alters two of those inputs, so it cannot be used as a final checker attestation.
The nine reviewed CI files still match their own frozen hashes at this review.

## Outstanding enforcement requirements

This is a repository-code checkpoint only. The current trigger covers main; delivery-branch
coverage for develop remains unfinished. GitHub documents pull_request_target in the default
branch context, so adding develop also requires a deliberate separation of trusted checker
revision and PR comparison base. No dedicated App producer, exclusive required-check source,
external bootstrap decision or hosted enforcement test is supplied by this increment. Current
GitHub permissions/settings were not changed or revalidated by this review. Maintainer self-merge
and zero mandatory GitHub approvals are preserved. Pinned runner/action/tool/dependency behavior,
reviewer authentication and external producer isolation remain explicit trust boundaries.

Reference: https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request_target
