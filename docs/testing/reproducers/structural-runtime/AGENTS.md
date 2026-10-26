# Structural runtime behavioral replay

## Responsibility

These are standalone consumers of production interfaces, built as ordinary executables.

- [consumer.rs](consumer.rs) exercises record publication with real durable I/O and concurrent readers/writers, deferred loading, candidate identity, and the real isolated plugin host.
- [plugin-fixture.rs](plugin-fixture.rs) is a disposable native plugin that negotiates the actual SDK protocol or deliberately fails, floods stderr, sends malformed framing, or times out.
- [cache-probe.rs](cache-probe.rs) operates independent conversations through negotiated daemon IPC and checks revisions, receipts, and replay reuse.

Use isolated XDG state and copy fixture data. Do not point mutating consumers at a person's daemon. Evidence and timings belong in the associated testing report; generated binaries and raw state stay outside this repository.

[bootstrap.py](bootstrap.py) opens the offline human TUI; [history-replay.py](history-replay.py) restarts copied governed runs. [cache-invalidation.py](cache-invalidation.py) and [invalidation-probe.rs](invalidation-probe.rs) test external-commit fencing and corruption rejection. [baseline-identity.rs](baseline-identity.rs) links the original production identity API for compatibility comparisons.

[registry-review.rs](registry-review.rs) measures production snapshot/lookup scaling. [cache-read-review.rs](cache-read-review.rs) reads real conversations repeatedly over IPC and records per-group latency, cache counters, and daemon RSS. These are bounded performance consumers, not unit suites.
## Working in this area

Start with the listed driver and its nearest campaign report to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
