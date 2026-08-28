# peritus-product-runner

Daemon-owned production composition for interactive Peritus coding runs. The adapter joins:

- the `peritus-agent` D0 provider/tool loop for real inspect, search, edit, command, test, and retry;
- `peritus-gates` changed-path planning and exact per-project compile/test/lint evidence;
- `peritus-review` typed policy-derived findings conserved through fixer and fresh-review cycles;
- `peritus-orchestrator` fail-closed E0 accept/fix/exhaust decisions; and
- a durable task candidate, provider/tool trace, task-level summary, and explicit deliverable
  handoff consumed by the daemon.

The crate consumes already resolved provider and managed-workspace capabilities. It emits bounded
progress and deliverable evidence; it does not own UI, provider login, workspace trust, or Git
commit/export/discard authority. `Complete` is impossible for an empty or uncovered candidate, a
failed or missing exact-target command, or any unresolved policy blocker.

Provider, filesystem, process, and Git effects remain ordinary Rust host adapters. In
`verus_only` builds the crate exposes the same daemon-facing boundary as a fail-closed total
implementation that cannot fabricate completion; the D0, D1, D2, and E0 decision cores remain in
their formally classified crates and the ordinary-API audit constrains the production adapter.

Focused qualification:

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-product-runner --all-targets --all-features --locked -- -D warnings
```
