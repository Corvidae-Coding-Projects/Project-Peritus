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

Focused checks:

```bash
CARGO_BUILD_JOBS=2 cargo test -p peritus-external-benchmarks --all-features
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-external-benchmarks --all-targets --all-features -- -D warnings
```

Generated traces and benchmark workspaces belong in local state outside Git. See
`benchmarks/external/README.md` for pinned upstream revisions and run commands.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-external-benchmarks
```
