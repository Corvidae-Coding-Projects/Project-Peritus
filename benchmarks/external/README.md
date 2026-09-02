# External agent benchmarks

Peritus uses two upstream suites as product diagnostics:

- HarnessBench measures orchestration, recovery, state handling, tool use, and verification.
- Terminal-Bench 2.0 measures whether the same harness can finish real tasks in isolated Unix
  environments.

The files in this directory pin upstream source and describe the native adapter. Generated suite
workspaces and results are deliberately stored outside Git. Run heavy suites one at a time with
`CARGO_BUILD_JOBS=2`.

Terminal-Bench campaign evidence is normalized by the Rust-owned
`peritus-terminalbench-report` command. Its versioned output follows
`../schemas/terminalbench-campaign-report-v1.schema.json`; it rejects a transient Harbor state in
which the root aggregate has advanced before the matching child result is visible. The completed
frozen diagnostic baseline contains all 445 trials and is described in
`terminalbench/README.md`; its final-candidate comparison has not run yet.

See `harnessbench/README.md` and `terminalbench/README.md` for pinned inputs, adapter boundaries,
resource-aware commands, and current qualification evidence.

`failure-journal.md` records each reproduced benchmark failure, its product cause, the change made,
and the result of the next unchanged benchmark run.
