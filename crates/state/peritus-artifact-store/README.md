# peritus-artifact-store

## Folder guide

Streaming content-addressed artifact storage for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-artifact-store`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Streaming content-addressed artifact storage for Peritus. |

Content-addressed artifact storage for Project Peritus.

## Write and reference contract

The crate provides bounded streaming writers with incremental SHA-256 verification. Finalization
checks exact size and digest, flushes and synchronizes the temporary file, publishes through an
atomic no-replace hard link, synchronizes directories, and records durable metadata. Existing
identical content is idempotent; an existing object that disagrees with the digest encoded by its
path is terminal corruption. All internal object and quarantine paths are derived from typed digest
bytes rather than caller-supplied path fragments.

Metadata records finalization, media type, optional encryption binding, creating event, quarantine
state, and durable integrity state. Only finalized, active, healthy objects can become journal or
evidence roots. For the C0
composition, use `StoreConfig::with_database_path` to select the same SQLite file as
`peritus-journal`; the standalone default is `metadata.sqlite3` below the artifact root.

## Recovery, quota, and collection

Opening the store runs idempotent restart recovery. It removes abandoned temporary files, verifies
cataloged bytes, finishes interrupted quarantine moves, quarantines a published object that missed
catalog insertion, and deletes an untracked quarantine file only on a later recovery pass. When a
cataloged object has divergent bytes, recovery durably marks it corrupt, moves it out of the active
namespace, retains its audit roots, and denies reads and new references. Reopening repeats that
state safely. Missing cataloged bytes and noncanonical layouts still fail closed.

Quota planning uses checked logical catalog bytes. Filesystem capacity is exposed as an
observation, not conflated with quota admission. Garbage collection is an explicit deterministic
mark/quarantine/restore/sweep plan over current journal and evidence roots: a newly unmarked object
is quarantined and becomes deletable only in a strictly later collection generation. Interrupted
actions are reconciled when the store is reopened.

See [C0 durable state](../../../docs/c0-durable-state.md) for lifecycle details, operator recovery,
and exact validation commands.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
