# peritus-gates

`peritus-gates` is the D1 deterministic acceptance-gate engine. It binds immutable B2 contracts to
exact revisions and clean C1 snapshots, invokes only authorized C4 quality checks, records every
transition through C0, and publishes exact-revision evidence before a gate can pass.

The pure reducer is the authority for scheduling and terminal truth. Process, artifact, evidence,
and storage adapters return observations; none may directly mutate gate state or infer success.
Execution and action identities are consumed once per run, resolved dispatches never recreate effect
permits, every engine is bound to one durable journal `StoreId`, and evidence receipts are bound to
one authoritative-journal result-publication manifest with one distinct admitted record per
requirement.

Public task acceptance can also be evaluated through the pure `GateObligationAssessment` bridge.
It admits a gate pass only when `peritus-obligations` qualifies every active, current requirement;
failure ownership is routed separately so only candidate defects request another fixer cycle.

The production-facing `TargetGatePlan` maps every changed path to the nearest Cargo, Node, Python,
Go, conventional SQLite, or explicit artifact project. Manifestless Python and Node projects are
recognized from ordinary test layouts, and standalone Python modules still receive syntax checks.
The plan creates explicit structured compile, test, lint, dependency, migration, and artifact
commands for each affected project. Changed CSV, JSON, and YAML files are structurally parsed by
bounded native gates. `TargetGateReport` passes only when candidate coverage is complete, every
planned command has one observation, and every exit code is zero. Rust commands use locked inputs,
all targets, and all features; unrelated root commands cannot cover a nested project.

See [`docs/d1-gate-engine.md`](../../../docs/d1-gate-engine.md) for the lifecycle and integration
contract.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-gates
```
