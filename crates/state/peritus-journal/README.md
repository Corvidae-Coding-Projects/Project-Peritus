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

Aggregate replay reads the head and all event rows in one read transaction. One ordered event
query replaces per-event queries while retaining exact frame/hash checks, contiguous predecessor
validation and complete final-head equality. Orphaned events, damaged historical bytes and
self-consistently hashed but disconnected records are rejected. This read-path optimization does
not introduce a checkpoint cache, alter canonical bytes or discard state history.

`aggregate_checkpoint_snapshot` also reads a requested current state row in that same transaction,
so a concurrent commit cannot mix old events with a newer checkpoint. The domain adapter must
still check the state row's identity and compare its contents against deterministic replay; this
snapshot is not permission to reuse cached state or authorize a later append.

`observe_replay` records a process-local journal instance/append generation and SQLite's external
commit version before cold replay. `append_observed` checks that observation again under the write
lock, then performs every normal append check. Another append attempt (including failure), journal
reopen, or external commit invalidates reuse. A successful receipt returns the next observation;
it retains the external version checked inside the transaction, never a newer post-commit value.
The observation covers append-owned history and checkpoints, not mutable application ledgers or
outbox leases, and it does not replace the domain's deterministic replay validation.

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

The initial release schema (version 1) retains immutable state revisions as lossless,
content-addressed trees of 512-byte
leaves with fanout 16. Historical reads reconstruct and verify every referenced node and then the
original state SHA-256 in one read snapshot. Missing nodes, digest mismatches, noncanonical shapes,
and missing producing events fail closed. Nodes and their root references commit in the same append
transaction as events and current state; reused nodes are checked, never silently overwritten.
There is one history format and no supported upgrade path from unshipped development schemas.

Each tree installation or reconstruction prepares its fixed SQL statements once within the
existing transaction. Recursive visits rebind and execute those statements, still reading and
validating every node on every visit. The statements are dropped before the operation returns;
no node bytes, validation results, or transaction authority are cached by this optimization.

This reduces repeated historical storage; it does not remove full current-state writes, canonical
encoding, or per-append traversal of the proposed tree. It is not a replay cache or an H3 performance
qualification result. All historical revisions remain retained; there is no node garbage collector.

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
