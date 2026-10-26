# Checkpoint and cache real replays

## Responsibility

These drivers own disposable fixture state; they never operate a live user workspace or installed
daemon. Follow [README.md](README.md) for builds, independent expectations, account-inference
requirements and Linux-specific measurement limits.

| File | Role |
| --- | --- |
| [README.md](README.md) | Replay prerequisites, commands, expected behavior and evidence boundaries. |
| [cohort.py](cohort.py) | Select the actual Cargo-linked dependency variants. |
| [link-consumers.py](link-consumers.py) | Link ordinary public-API consumers outside Cargo test membership. |
| [scope-consumer.rs](scope-consumer.rs) | Persistent public ScopedBaseline consumer with logical-read accounting. |
| [replay.py](replay.py) | Deterministic filesystem changes, recovery, types, permissions and concurrent readers. |
| [migration-replay.py](migration-replay.py) | Original-library baseline followed by current-library comparison. |
| [high-cardinality.py](high-cardinality.py) | Old NUL-preview SQLite index with 16,384 pages and bounded 64-MiB payload. |
| [index-invalidation.py](index-invalidation.py) | Another process's enrollment, new files and legacy-journal invalidation. |
| [cold-index-race.py](cold-index-race.py) | Alter owned legacy evidence during a real cold index scan. |
| [context-consumer.rs](context-consumer.rs) | Actual tools, current context assembly, scoped evidence and public entries. |
| [context-replay.py](context-replay.py) | Source reuse, pressure, exact freshness, permissions, isolation and stable history. |
| [mixed-batch-replay.py](mixed-batch-replay.py) | Keep unrelated settled results and assistant/tool ordering when a read goes stale. |
| [invocation-state.py](invocation-state.py) | Stable system/task, exact current developer state and complete current exchange. |
| [checkpoint-probe.rs](checkpoint-probe.rs) | Public daemon IPC inspection and selective rewind after restart. |
| [live-command.py](live-command.py) | Actual TUI, provider, commands, checkpoint persistence and recovery. |
| [live-memory.py](live-memory.py) | Actual agent provenance, resolution, supersession and archived readback. |
| [provider-consumer.rs](provider-consumer.rs) | Four real account requests against unchanged source and changed output markers. |
| [provider-recovery-consumer.rs](provider-recovery-consumer.rs) | Cache-disabled route, real model rejection and controlled anchor-loss continuation. |
| [archive-replay.py](archive-replay.py) | Archive the real probe's own anchor and require two correct fallback/continuation responses. |
| [process-consumer.rs](process-consumer.rs) | Real ordinary/queued child IO, overflow, cancellation, timeout and reaping. |
| [observed-results.json](observed-results.json) | Compact observed provider/process outcomes; no credentials or large payloads. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent](../AGENTS.md) · [Campaign](../../checkpoint-cache-real-2026-10-19.md).
