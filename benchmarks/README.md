# Peritus benchmarks

This directory contains stable benchmark inputs and adapters. Generated workspaces, downloaded
images, provider account state, traces, and results must stay in an external state directory.

## Product performance

The H3 qualification data is checked in so release candidates use the same workloads and rules:

- `profiles/qualification-candidate-v1.json` defines the AMD production reference-machine envelope
  and SLOs.
- `profiles/qualification-intel-core-ultra-9-275hx-v1.json` records the exact host used for the
  retained local qualification campaign. It keeps the same resource envelope and SLOs; it does not
  replace or weaken the production reference profile.
- `workloads/production-v1.json` defines deterministic load and eight-hour soak workloads.
- `schemas/` defines measurement, baseline, evidence, and report formats.
- `baselines/` remains empty until reviewed evidence establishes a real accepted baseline.

An absent baseline cannot produce a `ready` performance verdict. Runner output is evidence, not a
source fixture, and must never be written into this tree.

## External agent suites

- [HarnessBench](external/harnessbench/README.md) measures orchestration, recovery, state, tool use,
  and verification. Its 106-task retained campaign is complete.
- [Terminal-Bench 2.0](external/terminalbench/README.md) measures end-to-end work in isolated Unix
  environments. Its 89-task, five-attempt campaign is running serially to protect host memory.
- The [failure journal](external/failure-journal.md) records every reproduced defect, diagnosis,
  broadly useful fix, unchanged rerun, retained benchmark mismatch, and evidence locator.

Benchmark adapters must not change upstream tasks, fixtures, hooks, rubrics, images, or verifiers.
A benchmark reward and Peritus's native acceptance are recorded separately so neither can conceal a
failure in the other.
