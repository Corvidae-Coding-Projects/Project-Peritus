# Structural runtime behavioral replay

These are standalone consumers of production interfaces, built as ordinary executables.

- `consumer.rs` exercises record publication with real durable I/O and concurrent readers/writers, deferred loading, candidate identity, and the real isolated plugin host.
- `plugin-fixture.rs` is a disposable native plugin that negotiates the actual SDK protocol or deliberately fails, floods stderr, sends malformed framing, or times out.
- `cache-probe.rs` operates independent conversations through negotiated daemon IPC and checks revisions, receipts, and replay reuse.

Use isolated XDG state and copy fixture data. Do not point mutating consumers at a person's daemon. Evidence and timings belong in the associated testing report; generated binaries and raw state stay outside this repository.

`bootstrap.py` opens the offline human TUI; `history-replay.py` restarts copied governed runs. `cache-invalidation.py` and `invalidation-probe.rs` test external-commit fencing and corruption rejection. `baseline-identity.rs` links the original production identity API for compatibility comparisons.
