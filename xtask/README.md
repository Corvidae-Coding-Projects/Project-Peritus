# Peritus xtask

`xtask` is the dependency-light engineering-policy executable for the Peritus workspace. It owns
no product or authority decisions. It validates the Cargo dependency graph, package ownership and
verification-class metadata, source layout and size budgets, trusted-Verus construct placement,
ordinary-Rust formal API boundaries, toolchain pins, lockfile policy, and immutable GitHub
Actions references. The reproducibility check also locks the reviewed GitHub Team repository-
ruleset activation template; live GitHub enforcement is verified separately through the documented
API runbook because an offline repository process cannot attest to external state.

The same check locks the default-branch `formal-authority.yml` workflow byte-for-byte and validates
its exact `main`/`develop` trigger, permissions, separate checker/base/candidate SHA custody,
pre-metadata execution guards, both checker-input equality edges, trusted checker build, offline
candidate evaluation, and ordered `all` plus `verify-trust` calls. The proof-impact evaluator stays
bound to the exact PR base. Checker or dependency transitions require a separately controlled
exact-head bootstrap that establishes identical protected inputs on `main` and `develop`; candidate
records cannot authorize them. This is a repository-code validation contract. Exclusive
required-check authority still depends on the separately deployed and source-bound GitHub App
described in `docs/github-governance.md`.

`docs-check` inventories maintained Markdown, checks basic structure and local links, and requires
each crate README to name its focused test command. This keeps documentation failures visible in
the same local and CI gate as source-layout and architecture failures.

`format-check` runs Rustfmt once per workspace package in deterministic package-name order. This
preserves the complete workspace formatting gate without exceeding the Windows process command-line
limit as the repository grows.

The checks return stable error categories and actionable diagnostics. CI and `just check` invoke
`cargo run --locked --package xtask -- all` directly so a repository-controlled Cargo alias cannot
swallow bootstrap evidence. Once the root Cargo configuration has passed policy, `cargo xtask all`
is the equivalent developer convenience interface.

An authorization-only commit preserves the comparison base's application, policy, and actor
files. On that commit, `all` validates the immutable authorization and runs the complete local
policy suite against its exact materialized implementation candidate. That nested candidate checks
its actors, obligations, and trust boundaries without recursively selecting historical proof-impact
authorization. Canonical workflow resources live under `xtask/src/reproducibility/canonical` so the
reviewed checker builds on the unchanged base; candidate workflow bytes must still match them.
`verify-trust` additionally enforces the externally selected review base and approved transitions.

## Focused checks

`formal-inventory` renders the declared obligation, trust, and exclusion registers alongside
all formal packages and their last available local compiler-scope observations. The JSON is an
audit aid: missing observations and observed zero-query packages remain distinct, and it does
not authorize proof discharge, assert report freshness, or establish production correspondence.
See the [active coverage audit](../docs/formal-coverage-audit.md) for current limits and work.

`proof-impact-inventory` emits raw-byte fingerprints and affected package sets using the trust
gate's own compilation-source discovery and ownership policy. It can inspect a checkout whose
approval inventory is stale without treating that stale manifest as authority. Its `audit-only`
output does not approve transitions, run evidence commands, or discharge obligations. Discovery
errors still fail the command. Compare it against the actual protected Git base and retained
history before preparing an independently reviewed authorization.

The existing `model-orchestration` build, test, and Clippy shards also run a separate
`peritus-agent --all-targets --no-default-features` check after their normal all-feature command.
Separate Cargo invocations preserve the bridge-disabled feature graph even when other workspace
packages depend on the default protocol bridge. Failure of either configuration fails the shard.

Foundation and Gate A run `ci-shard test-daemon app-shell` separately from the
ordinary `ci-shard test app-shell` job on Linux, macOS, and Windows. Together
they select every application-shell package exactly once, with locked dependencies,
all targets and features, serial tests, and the unchanged ten-minute job limit.
Daemon build, Clippy, documentation, and Verus checks remain in the existing
application-shell shards. Exact workflow checks reject missing, repeated, or
misrouted daemon jobs.

Native product CI builds each of its seven native binaries in a separate bounded job on each
platform, then assembles the downloaded binaries in a separate preparation job. Artifact names
separate the platform and binary with a double hyphen so ARM and Intel macOS downloads cannot
overlap. Assembly requires every binary and restores Unix executable permissions before use.
The public installer lifecycle and all 18 H2 scenarios consume that platform's same-run prepared
artifact without rebuilding the application. The small xtask entry point still uses the
reviewed locked Cargo command; its prepared execution path never invokes Cargo. Missing artifacts
fail rather than triggering a rebuild. The local build-and-qualify commands remain available, and
these native product CI jobs retain their ten-minute limit.

Hosted jobs default to a ten-minute ceiling. Only six named compilation jobs in
`.github/workflows/release.yml` have a twenty-minute allowance: `build-daemon-library`,
`build-cli-library`, `build-binary`, `check-native-staging`, `distro-compile`, and
`distro-compile-checks`. This includes setup and artifact retention, not just Cargo
execution. Policy rejects larger limits, other job names, and the same names in
other workflows. Qualification, assembly, signing, and publication checks keep
their existing ten-minute limits and requirements.

Native release and bootstrap archives require Python 3.12+ on the build host
(`python3` on Unix, `python` on Windows), not on the installed product host.
The archive writer fixes entry order, owner/group, normalized permissions, and
timestamps at the committed source epoch; it rejects links, special files, and
existing output files. The focused test command below exercises both tar/gzip
and ZIP bytes. This removes archive metadata variability; an independent native
rebuild must still prove the complete candidate outputs byte-identical for H4.

The [native release compilation boundary](../packaging/native-build.md) keeps
Intel macOS daemon library, CLI, and daemon binary compilation in separate
same-role jobs. Source/environment-bound library transport preserves each binary's
own Cargo feature graph, and assembly verifies both archived binaries' evidence.

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package xtask
cargo xtask docs-check
```
