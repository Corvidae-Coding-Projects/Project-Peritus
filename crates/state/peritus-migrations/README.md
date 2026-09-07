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

The production registry contains ten contiguous descriptors retaining their original release
identity `0.0.0`; shipping package version `0.0.1` does not rewrite applied migration history. Version 1
establishes the original migration marker. Version 2 rebuilds the journal head and event tables to
admit the permanent D0 `Agent` aggregate tag. Version 3 performs the same byte-preserving,
count-checked table replacement for the permanent D1 `Gate` and C7 `Trace` tags. Version 4 repeats
the byte-preserving replacement to admit the permanent D2 `Review` tag. Version 5 admits the D3
`Scheduler` and `Collaboration` tags plus the E0 `Orchestrator` tag in one byte-preserving
replacement. Version 6 repeats the constrained-table copy to admit the E1 `Harness` tag while
preserving tags 1–12. Version 7 performs the same byte-preserving replacement to admit the E2
`Debugger` tag while preserving tags 1–13. Version 8 admits E3 `Evaluation` (tag 15).
Version 9 admits F0 `EvolutionCampaign` and `ProductionHarness` (tags 16–17).
Version 10 admits G0 `Application` (tag 18) and installs its principal, session, command,
prompt-target, artifact, and workspace persistence tables and indexes. Every step is exact-source
SHA-256 checked and backup-required; version 1 declares 64 KiB of scratch and all nine later steps
declare 32 MiB. A successful complete upgrade publishes both `store_meta.schema_version` and
`PRAGMA user_version` as 10.

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
14, and restores the exact v6 backup. The v7 and v8 fixtures likewise preserve historical rows,
admit E3 and F0 aggregates, and restore their original backups. The v9 fixture verifies that the
version-10 application schema exactly matches a fresh journal installation, preserves all 17
historical aggregate families, and disappears again when the v9 backup is explicitly restored.
Artifact, projection, and evidence schemas remain owned by their adapters, so a migration version
alone does not assert that every component schema exists.

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
