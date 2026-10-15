# Peritus benchmarks

## Folder guide

Benchmark profiles, baseline records, and external-suite integration notes. These are separate from routine real-testing workflows.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [baselines/](baselines/AGENTS.md) | Baseline admission notes; this directory currently has no accepted baseline data. Benchmark results live outside the source tree. |
| [external/](external/AGENTS.md) | External benchmark adapters, pinned upstream identities, retained failure notes, and protocol/report fixture inputs. |
| [profiles/](profiles/AGENTS.md) | Retained performance/SLO workload profiles and platform resource selections. Read the consuming evaluation code for enforcement. |
| [schemas/](schemas/AGENTS.md) | JSON contracts for benchmark reports, profile inputs, comparison baselines, and campaign identities. |
| [workloads/](workloads/AGENTS.md) | Checked-in workload identities and parameters consumed by benchmark/evaluation tools. |

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
  environments. Its 89-task, five-attempt frozen diagnostic campaign completed all 445 serialized
  trials; broadly useful repairs and the exact final-candidate rerun remain.
- The [failure journal](external/failure-journal.md) records every reproduced defect, diagnosis,
  broadly useful fix, unchanged rerun, retained benchmark mismatch, and evidence locator.

Benchmark adapters must not change upstream tasks, fixtures, hooks, rubrics, images, or verifiers.
A benchmark reward and Peritus's native acceptance are recorded separately so neither can conceal a
failure in the other.
