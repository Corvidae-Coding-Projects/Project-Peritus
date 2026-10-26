# Token-budget removal replays

## Responsibility

These consumers operate the actual context and product-runner libraries. They do not implement
a model, replace a provider, or dispatch application commands.

| File | Role |
| --- | --- |
| [README.md](README.md) | Observable contracts, isolated setup, commands, and coverage boundaries. |
| [consumer.rs](consumer.rs) | Source-backed working-state rendering, concrete numeric failure, generated states, restart, and role isolation. |
| [exact-boundary-fixture.json](exact-boundary-fixture.json) | Concrete input that reproduced the 16,709 / 16,903 token rejection. |
| [host_consumer.rs](host_consumer.rs) | Copied long-run archive recovery, actual host assembly/checkpoint, Codex transport fitting, and configuration compatibility. |
| [prepare_host.py](prepare_host.py) | Prepare isolated state from an operator-created closed backup and disposable workspace copy; retain only non-secret provider metadata. |
| [build_consumer.py](build_consumer.py) | Build either consumer outside the checkout with two Cargo workers and the production release profile. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent](../AGENTS.md) · [Evidence report](../../token-budgets-real-2026-10-16.md).
