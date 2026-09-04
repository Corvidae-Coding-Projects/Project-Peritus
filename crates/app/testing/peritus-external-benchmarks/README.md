# Peritus external benchmarks

This crate is the native boundary between Peritus and third-party agent benchmarks. It runs the
real repository-grounded designer, writer, checks, reviewer, and fixer without starting the TUI or
daemon.

HarnessBench calls `peritus-benchmark-agent harnessbench` through its unchanged generic command
adapter. Harbor calls `peritus-benchmark-agent terminalbench` through the checked-in Terminal-Bench
custom agent. Both commands use the same native product composition. The benchmark suites remain
responsible for fixtures, timeouts, oracles, scoring, and retained workspaces.

Each benchmark run also opens the production G4 command runtime against its private workspace and
an isolated C2 state root. Benchmark tasks therefore exercise the same synchronous and active C4
command controls as the interactive product; the Python adapters only upload and invoke the native
binary. No benchmark-specific shell or PTY implementation substitutes for the harness.

Both commands emit the same schema-version-6 invocation report. It records the adapter handshake,
source and binary identity, suite revision, provider routes and live-canary status, exact candidate
digest and changed paths, verified native disposition, gates, obligations, review, terminal cause,
trace, resource use, and token/cache accounting. Upstream reward and verifier exceptions remain
separate fields that never influence native acceptance.

Every attributable live invocation requires `PERITUS_SOURCE_REVISION` at compile time. Ordinary
workspace builds may omit it, but an admitted run then emits an explicit failed identity report
instead of disappearing. `peritus-benchmark-agent protocol` exposes the source, executable digest,
and report schema without contacting a provider. `peritus-benchmark-agent qualify-providers` then
sends one minimal real request through both configured account runtimes before expensive work.

Admission prepares the workspace, trace, evidence, and separate recovery paths before model work.
After admission an unconditional settlement guard emits exactly one atomic `invocation.json`, or a
recovery report if primary publication fails. Product errors, timeouts, cancellation, trace
projection failures, and unwinds therefore remain scoreable. Only the verified `accepted`
disposition sets `success=true`; a retained candidate never becomes native or upstream success by
itself.

Trace projection validates response, tool, compaction, retry, and provider-switch frames. Retry or
switch metadata closes an incomplete projected response while remaining separate from the
conversation transcript, so diagnostics stay readable without fabricating a successful model
round.

## Generic capability regressions

The test-only `tests/fixtures/general-capability/` tree reproduces completion, selective resume,
schema, malformed-HTML, provider, repository, and adapter failure classes without naming an
upstream benchmark task or encoding a hidden verifier answer. Every family includes a successful
case, an honestly incomplete case, and a terminal failure. The assertions use native settlement,
provider, obligation, workspace, and publication values rather than matching model prose.

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
