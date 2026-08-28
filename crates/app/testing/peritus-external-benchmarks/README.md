# Peritus external benchmarks

This crate is the native boundary between Peritus and third-party agent benchmarks. It runs the
real repository-grounded designer, writer, checks, reviewer, and fixer without starting the TUI or
daemon.

HarnessBench calls `peritus-benchmark-agent harnessbench` through its unchanged generic command
adapter. Terminal-Bench support will use the same binary through Harbor's custom-agent interface.
The benchmark suites remain responsible for fixtures, timeouts, oracles, scoring, and retained
workspaces.

Focused checks:

```bash
CARGO_BUILD_JOBS=2 cargo test -p peritus-external-benchmarks --all-features
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-external-benchmarks --all-targets --all-features -- -D warnings
```

Generated traces and benchmark workspaces belong in local state outside Git. See
`benchmarks/external/README.md` for pinned upstream revisions and run commands.
