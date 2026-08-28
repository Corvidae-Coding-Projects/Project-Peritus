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

The production-facing `TargetGatePlan` maps every changed path to the nearest Cargo, Node, Python,
or Go manifest and creates explicit structured compile/test/lint commands for each affected
project. `TargetGateReport` passes only when candidate coverage is complete, every planned command
has one observation, and every exit code is zero. Rust commands use locked inputs, all targets, and
all features; unrelated root commands cannot cover a nested project.

See [`docs/d1-gate-engine.md`](../../../docs/d1-gate-engine.md) for the lifecycle and integration
contract.
