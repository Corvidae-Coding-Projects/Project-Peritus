# Multiple terminal windows — real replay

## Responsibility

| File | Role |
| --- | --- |
| [README.md](README.md) | Disposable-state prerequisites, independent expectations, commands, and coverage limits. |
| [replay.py](replay.py) | Opens actual Peritus clients through Linux PTYs, performs setup/permission inspection/reconnection, observes daemon identity, and retains screens and results. Optional cases use an actual configured Codex account or inject invalid catalog files into the owned fixture. |

Implementation owners: [launcher supervision](../../../../crates/app/peritus-launcher/src/daemon/AGENTS.md), [live catalog](../../../../crates/app/peritus-daemon/src/product_run/workspace_catalog.rs).

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Replay resources](../AGENTS.md) · [Repository](../../../../AGENTS.md).
