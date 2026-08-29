# peritus-process

`peritus-process` is Project Peritus's sole authorized operating-system execution boundary. It
binds structured commands to committed B0/B1/B3/C0 authority, persists one-use execution intent,
owns pipe or PTY process trees and every support thread, bounds input/output/events, and records a
deterministic terminal result suitable for restart reconciliation.

The crate deliberately exposes no shell-command parser and no public raw spawn API. Native sandbox
backends implement the frozen launch boundary in C3; the C2 local owner provides real process and
PTY lifecycle semantics and fails closed when a requested containment mode is unsupported.

Execution plans project the checked sandbox's terminal permissions, output/event limits,
environment-value provenance, resource ceilings, and admitted backend identity into the plan
digest. Linux local execution samples the owned process group for CPU, memory, process count, open
handles, and disk growth, terminates on observed overruns, and records sampled fidelity honestly.
Other platforms may run separately authorized raw-effect/reference plans with unsupported resource
observations, but a backend that claims supervisor or hard enforcement is rejected before durable
consumption when that support is unavailable.

The process store durably binds claims, lifecycle, terminal results, complete eight-dimension
resource observations, and per-stream artifact-publication progress. `wait_and_publish` returns a
publication error carrying the latest durable terminal result, while
`ProcessStore::retry_artifact_publication` resumes only missing streams and is idempotent across
restart.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-process
```
