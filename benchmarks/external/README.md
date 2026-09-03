# External agent benchmarks

Peritus uses two upstream suites as product diagnostics:

- HarnessBench measures orchestration, recovery, state handling, tool use, and verification.
- Terminal-Bench 2.0 measures whether the same harness can finish real tasks in isolated Unix
  environments.

The files in this directory pin upstream source and describe the native adapter. Generated suite
workspaces and results are deliberately stored outside Git. Run heavy suites one at a time with
`CARGO_BUILD_JOBS=2`.

## Current phase: failure remediation

Both complete diagnostic suites are frozen. Do not start, resume, or replace either campaign while
their failures are being converted into general product corrections. Use the retained reports,
workspaces, traces, observations, and unchanged verifier output to establish each cause; validate a
correction with focused synthetic regressions and normal repository gates. A new full comparison
belongs after the remediation code is frozen and the operator explicitly starts qualification, not
after each individual fix.

Terminal-Bench campaign evidence is normalized by the Rust-owned
`peritus-terminalbench-report` command. Its versioned output follows
`../schemas/terminalbench-campaign-report-v1.schema.json`; it rejects a transient Harbor state in
which the root aggregate has advanced before the matching child result is visible. The completed
frozen diagnostic baseline contains all 445 trials and is described in
`terminalbench/README.md`; its final-candidate comparison has not run yet.

See `harnessbench/README.md` and `terminalbench/README.md` for pinned inputs, adapter boundaries,
resource-aware commands, and current qualification evidence.

`failure-journal.md` records each reproduced benchmark failure, its product cause, the general
change made, focused regression evidence, and any later unchanged benchmark observation that
already exists.

The [diagnostic failure report](uncorrected-failures-report.md) gives the current decision-oriented
view without altering official scores. Its
[HarnessBench inventory](harnessbench-failure-inventory.md) accounts for all 106 tasks, and its
[Terminal-Bench inventory](terminalbench-failure-inventory.md) accounts for all 445 trials. The
[adverse-trial ledger](terminalbench-adverse-trials.md) names every individual trial with an
official, native, or exception failure signal.

The codebase-grounded [remediation design](../../.design/benchmark-failure-remediation.md) maps
every remediable failure class to a general product correction, explicit non-cooking boundary,
parallel implementation slice, and fixed-build acceptance criterion.
