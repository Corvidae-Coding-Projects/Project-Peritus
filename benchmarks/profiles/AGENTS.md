# benchmarks/profiles — navigation

## Responsibility

Retained performance/SLO workload profiles and platform resource selections. Read the consuming evaluation code for enforcement.

## Files in this folder

| File | Role |
| --- | --- |
| [qualification-candidate-v1.json](qualification-candidate-v1.json) | Candidate production SLO profile. Baseline comparison remains mandatory before a ready verdict. |
| [qualification-intel-core-ultra-9-275hx-v1.json](qualification-intel-core-ultra-9-275hx-v1.json) | Measured Project Peritus qualification host. It preserves the production candidate SLOs and does not replace the AMD reference profile. |

## Working in this area

Read the profile or workload definition together with the runner that consumes it and the baseline metadata that identifies the compared build. Benchmark artifacts measure the stated scenario only; keep them separate from product acceptance and refresh provenance whenever inputs or methodology change.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
