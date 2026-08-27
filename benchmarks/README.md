# Stable H3 qualification datasets

This directory contains inputs, never generated benchmark output.

- `profiles/qualification-candidate-v1.json` is the candidate reference-machine envelope and SLO
  contract. It requires an accepted baseline, so the absence of baseline evidence cannot produce a
  `ready` verdict.
- `workloads/production-v1.json` contains deterministic load and eight-hour soak definitions. Seeds,
  rates, concurrency, payloads, reservations, and queue capacities are part of the stable dataset.
- `schemas/` contains the version-1 interchange contracts for profiles, workload catalogs,
  measurements, baselines, evidence manifests, and qualification reports.
- `baselines/` deliberately contains no claimed performance result. An accepted baseline is written
  only from retained H3 evidence, reviewed, and content-bound to its evidence manifest digest.

Runner output belongs in an external evidence directory selected by the qualification invocation. It
must not be written back into this source dataset tree.
