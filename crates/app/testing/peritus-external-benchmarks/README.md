# Peritus external benchmarks

This crate is the native boundary between Peritus and third-party agent benchmarks. It runs the
real repository-grounded designer, writer, checks, reviewer, and fixer without starting the TUI or
daemon.

HarnessBench calls `peritus-benchmark-agent harnessbench` through its unchanged generic command
adapter. Harbor calls `peritus-benchmark-agent terminalbench` through the checked-in Terminal-Bench
custom agent. Both commands use the same native product composition. The benchmark suites remain
responsible for fixtures, timeouts, oracles, scoring, and retained workspaces.

The Terminal-Bench command records a flat schema-versioned report plus the native trace, last
product observation, generated design, conversation state, provider identities, changed paths, and
aggregated token/cache accounting. A product rejection is still a completed benchmark attempt, so
Harbor's independent verifier—not the adapter—owns the external score.

Every live invocation requires `PERITUS_SOURCE_REVISION` at compile time. The report binds that
full Git object ID to the Cargo package version and the SHA-256 of the executable that actually ran.
Harbor independently hashes the uploaded binary and rejects a mismatched native report. Ordinary
workspace builds can compile the crate without this variable, but such a binary refuses to run an
external benchmark rather than publishing unattributable evidence.

Trace projection validates response, tool, compaction, retry, and provider-switch frames. Retry or
switch metadata closes an incomplete projected response while remaining separate from the
conversation transcript, so diagnostics stay readable without fabricating a successful model
round.

Generated traces and benchmark workspaces belong in local state outside Git. See
`benchmarks/external/README.md` for pinned upstream revisions and run commands.

`peritus-terminalbench-report` turns one retained Harbor job into a versioned campaign report. It
counts only direct child trial results, checks that count against Harbor's root job state, keeps
the nested verifier reward as the score, and records each native and verifier evidence path. It
infers source identity from consistent Harbor trial metadata and reports identity coverage; it
never accepts an operator-supplied source guess. Use `snapshot` while a campaign is running and
`final` only after all expected trials are visible and Harbor has marked the job finished.
Publication is atomic and refuses to replace an existing report.

`peritus-harnessbench-report` does the same for one complete HarnessBench campaign. It compares the
pinned task catalog with retained results, selects the newest result for each task by modification
time and then path, and records every selected path and SHA-256. It rejects missing or extra tasks,
malformed scores, unavailable token evidence, inconsistent task/session identities, and token
accounting errors. `allow-legacy` accurately reports older campaigns that predate native build
identity; `require-native` requires source revision and executable digest evidence for every task.
Publication is atomic and never overwrites an existing result.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-external-benchmarks --all-features
CARGO_BUILD_JOBS=2 cargo clippy --locked --package peritus-external-benchmarks \
  --all-targets --all-features -- -D warnings
```
