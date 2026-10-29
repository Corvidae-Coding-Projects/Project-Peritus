# Read reuse replay navigation

## Responsibility

These drivers operate actual public product tools, durable context, and the memory-tool decorator. They do not substitute a read implementation or provider response. The Python fixture owner supplies source bytes and expected state transitions independently.

| File | Role |
| --- | --- |
| [consumer.rs](consumer.rs) | Interactive native consumer, reusing the existing context consumer's production runtime setup. |
| [link.py](link.py) | Link that consumer against the freshly built production dependency cohort. |
| [replay.py](replay.py) | Concrete and seeded read/edit/compaction/restart/permission/role scenarios. |
| `README.md` | Contract, replay commands, and evidence limits. |
| [observed-results.json](observed-results.json) | Compact final scenario, byte-count, source-hash, and live-run receipts. |
| [regressions.json](regressions.json) | Concrete early-build failures, stable expectations, and replay command. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Reproducers](../AGENTS.md).
