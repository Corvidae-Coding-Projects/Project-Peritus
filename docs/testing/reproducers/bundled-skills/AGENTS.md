# Automatic skill bundle replay — navigation

## Responsibility

| File | Role |
| --- | --- |
| [README.md](README.md) | Exact replay command, owned fixtures, independent expectations and evidence scope. |
| [consumer.rs](consumer.rs) | Native consumer of the actual compiled catalog and production paginated readers. |
| [replay.py](replay.py) | Source-based oracle and seeded add/change/remove/source-independent behavioral campaign. |

Production owners: [canonical skills](../../../../skills/AGENTS.md),
[build discovery](../../../../crates/app/peritus-product-runner/build.rs), and
[compiled runtime](../../../../crates/app/peritus-product-runner/src/bundled_skills.rs).

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Reproducer map](../AGENTS.md) · [Testing evidence](../../AGENTS.md).
