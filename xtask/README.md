# Peritus xtask

`xtask` is the dependency-light engineering-policy executable for the Peritus workspace. It owns
no product or authority decisions. It validates the Cargo dependency graph, package ownership and
verification-class metadata, source layout and size budgets, trusted-Verus construct placement,
ordinary-Rust formal API boundaries, toolchain pins, lockfile policy, and immutable GitHub
Actions references. The reproducibility check also locks the reviewed GitHub Team repository-
ruleset activation template; live GitHub enforcement is verified separately through the documented
API runbook because an offline repository process cannot attest to external state.

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

## Focused checks

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
every hosted job retains its ten-minute limit.

Native release and bootstrap archives require Python 3.12+ on the build host
(`python3` on Unix, `python` on Windows), not on the installed product host.
The archive writer fixes entry order, owner/group, normalized permissions, and
timestamps at the committed source epoch; it rejects links, special files, and
existing output files. The focused test command below exercises both tar/gzip
and ZIP bytes. This removes archive metadata variability; an independent native
rebuild must still prove the complete candidate outputs byte-identical for H4.

The [native release compilation boundary](../packaging/native-build.md) keeps
Intel macOS daemon library and binary compilation in separate same-role jobs,
with source/environment-bound transport and archived-binary evidence checks.

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package xtask
cargo xtask docs-check
```
