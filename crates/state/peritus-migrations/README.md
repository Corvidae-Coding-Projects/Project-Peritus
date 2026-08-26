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

The production registry contains seven contiguous descriptors for release `0.0.0`. Version 1
establishes the original migration marker. Version 2 rebuilds the journal head and event tables to
admit the permanent D0 `Agent` aggregate tag. Version 3 performs the same byte-preserving,
count-checked table replacement for the permanent D1 `Gate` and C7 `Trace` tags. Version 4 repeats
the byte-preserving replacement to admit the permanent D2 `Review` tag. Version 5 admits the D3
`Scheduler` and `Collaboration` tags plus the E0 `Orchestrator` tag in one byte-preserving
replacement. Version 6 repeats the constrained-table copy to admit the E1 `Harness` tag while
preserving tags 1–12. Version 7 performs the same byte-preserving replacement to admit the E2
`Debugger` tag while preserving tags 1–13. Every step is exact-source SHA-256 checked and
backup-required; all six
table-rebuilding steps declare 32 MiB of scratch
space. A successful complete upgrade publishes both `store_meta.schema_version` and
`PRAGMA user_version` as 7.

The v1 compatibility fixture is migrated through all later descriptors, compared field-for-field
afterwards, checked with SQLite foreign-key validation and the journal integrity scanner, and then
extended with `Agent`, `Gate`, `Trace`, `Review`, `Scheduler`, `Collaboration`, `Orchestrator`, and
`Harness`
and `Debugger` records. A frozen v3 fixture additionally
proves tags 1–8 and their event frames remain byte-exact across the backup-required D2 migration
and rollback restoration. A frozen v4 fixture proves tags 1–9 remain byte-exact across the D3/E0
migration, validates new tags 10–12, and restores the exact v4 backup. A frozen v5 fixture proves
tags 1–12 remain byte-exact across the E1 migration, admits tag 13, and restores the exact v5
backup. A frozen v6 fixture proves tags 1–13 remain byte-exact across the E2 migration, admits tag
14, and restores the exact v6 backup. Journal, artifact, projection, and evidence
schemas are still installed by their owning adapters, so a migration version alone does not assert
that every owning schema exists.

At restart, `MigrationEngine::reconcile` classifies incomplete operations as resume-backup,
resume-apply, retry-apply, reconciled-applied, or restore-backup. It does not silently apply or
restore. Resume with the same operation identity, and make backup restoration an explicit operator
decision after preserving the failed database and recovery evidence.

See [C0 durable state](../../../docs/c0-durable-state.md) for startup ordering, backup/restore
details, and exact validation commands.
