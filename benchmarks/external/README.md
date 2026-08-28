# External agent benchmarks

Peritus uses two upstream suites as product diagnostics:

- HarnessBench measures orchestration, recovery, state handling, tool use, and verification.
- Terminal-Bench 2.0 measures whether the same harness can finish real tasks in isolated Unix
  environments.

The files in this directory pin upstream source and describe the native adapter. Generated suite
workspaces and results are deliberately stored outside Git. Run heavy suites one at a time with
`CARGO_BUILD_JOBS=2`.

See `harnessbench/README.md` for the current HarnessBench commands. Terminal-Bench instructions are
added after Harbor and the dataset are pinned and their custom-agent boundary is verified.

`failure-journal.md` records each reproduced benchmark failure, its product cause, the change made,
and the result of the next unchanged benchmark run.
