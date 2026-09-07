# peritus-migrations

Forward-only migrations for the shared Peritus SQLite store.

## Contract

The crate owns an immutable contiguous registry whose exact SQL sources are SHA-256 checked at
runtime. Preflight verifies registry and applied-history digests, SQLite integrity, forward-only
target ordering, application compatibility, and checked database/backup capacity. The migration
engine requires an existing regular database file, takes an exclusive owner lock, and applies all
selected steps in one exclusive SQLite transaction.

Risky plans create a consistent pre-migration SQLite backup in an exclusive temporary file, sync
and atomically publish it, and persist its digest before SQL runs. Migration operation identity,
registry digest, backup state, applied source digest, and release are durable. Reverse SQL is never
run; rollback is an explicit digest-verified backup restore.

## Current registry and recovery

Release `0.0.1` has one initial journal schema, version 1. The journal creates the complete schema
directly, including every release aggregate kind, application table, and shared-history storage.
`adopt_current_install` records its single release marker once without running migrations or
creating a backup. The development-era migration chain and its compatibility fixtures are removed;
unshipped databases are not supported upgrade targets.

The registry retains one exact-source-checked initial marker. Future released schema changes can
append forward migrations. Generic transaction, backup-integrity, interrupted-operation, and
restore tests remain because they check storage safety, not historical user compatibility.
Artifact, projection, and evidence schemas remain owned by their adapters.

Fresh-install tests verify the initial version and complete table inventory, one-time adoption,
all 18 aggregate families, shared history, and restart integrity.

At restart, `MigrationEngine::reconcile` classifies incomplete operations as resume-backup,
resume-apply, retry-apply, reconciled-applied, or restore-backup. It does not silently apply or
restore. Resume with the same operation identity, and make backup restoration an explicit operator
decision after preserving the failed database and recovery evidence.

See [C0 durable state](../../../docs/c0-durable-state.md) for startup ordering, backup/restore
details, and exact validation commands.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-migrations
```
