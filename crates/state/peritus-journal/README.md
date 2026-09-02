# peritus-journal

Transactional exact-byte event persistence, durable compare-and-append semantics, integrity
checking, and opaque commit receipts for Project Peritus.

## Contract

The crate validates bounded append requests without I/O, stores complete canonical B3 frames
without reserialization, and applies event, head, state, registry, artifact-reference, command,
and outbox rows in one `SQLite` transaction. File-backed journals use WAL mode,
`synchronous=FULL`, foreign-key enforcement, defensive mode, explicit runtime limits, and a
bounded busy timeout.

Committed batches are move-only observations with private construction. Lost acknowledgements are
resolved under the original command identity and request digest, and integrity exports are
available only after recomputing frame, event-chain, command-range, state, registry, artifact, and
head checks.

The journal is the authoritative transition history. Its state records, authority clock,
credential registry, and outbox are updated under checked compare-and-swap preconditions; query
projections remain replaceable consumers. The artifact catalog must be in the same SQLite file for
artifact-reference checks and row insertion to share the append transaction.

## Durable domain adapters

The crate owns move-only commit adapters for accepted B0 kernel transitions and the B1 capability,
budget, lease, approval, credential-registry, and authority-clock boundaries. B0 recovery replays
the stored envelope, command, and input capsules through a caller-supplied verified reducer driver
and compares every emitted frame and successor-state digest. Held-budget cancellation additionally
requires an opaque current `NonActivationObservation` from the journal.

Signed approval commits match both the current registry revision and the exact snapshot digest.
The durable registry row's global generation is retained separately from the signer credential's
generation; those values are not required to be equal.

These observations prove only that the exact post-commit state was observed. They are neither
signatures nor effect permissions, and raw B0/B1 values cannot construct them.

## Recovery

On an indeterminate append, retain the original command identity and request digest and call
`SqliteJournal::resolve_command`. A committed result, definite absence, and conflicting reuse are
distinct durable outcomes. Run `integrity_scan` or obtain an `integrity_export` before replay,
projection rebuild, or evidence admission; do not repair a broken chain by rewriting history.

See [C0 durable state](../../../docs/c0-durable-state.md) for composition, startup ordering,
failure classes, and validation commands.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-journal
```
