# C0 durable state

C0 is the durable state boundary for Peritus. It records already-checked B0 and B1 transitions,
derives replaceable query projections, manages content-addressed artifacts, evolves the shared
SQLite database, and admits revision-bound evidence. It does not run tools, workspaces, providers,
processes, or any other external effect.

This document describes the library APIs and recovery behavior implemented in the five C0 crates.
There is not yet a daemon, operator CLI, or automated startup supervisor that composes these steps.
Applications embedding C0 must preserve the ordering below and must supply deployment-specific
paths, identities, retention policy, and outage coordination.

## Durable ownership

| Component | Durable data | Authority |
|---|---|---|
| `peritus-journal` | Exact B3 frames, aggregate hash chains and heads, commands, state records, authority state, credential registry, outbox, and journal artifact references | Authoritative transition history |
| `peritus-artifact-store` | SHA-256-addressed object files plus artifact metadata and journal/evidence roots | Authoritative artifact bytes and reference state |
| `peritus-projection` | Versioned encoded projection generations and active-generation pointers | Rebuildable query state only |
| `peritus-migrations` | Applied migration descriptors and durable recovery-operation state, plus consistent backups | Schema compatibility and recovery control |
| `peritus-evidence` | Immutable evidence records, causal links, invalidations, and evidence artifact roots | Authoritative evidence catalog, derived from journal provenance |

The journal is the source of truth for state transitions. A projection is never an input to journal
repair, and a projection value is not a durable transition receipt. Artifact objects are durable
only when their bytes and metadata agree; only finalized active artifacts may be referenced.
Evidence is immutable history, but its currentness is a separate revision and invalidation decision.

`ArtifactStore::read` is the bounded materialization read boundary. It requires active finalized
metadata, rejects a caller limit below the durable size, reads a regular object file, and verifies
that the exact bytes returned still match both the recorded size and SHA-256 digest.

The shared journal schema is currently version 6. Its closed aggregate-kind registry includes the
permanent D0 `Agent`, D1 `Gate`, C7 `Trace`, D2 `Review`, D3 `Scheduler`/`Collaboration`, and E0
`Orchestrator` and E1 `Harness` kinds in addition to the foundational kernel and B1 state kinds.
Upgrades from
version 1 preserve existing event and head rows exactly while version 2 admits `Agent`, version 3
admits `Gate` and `Trace`, version 4 admits `Review`, version 5 admits the D3/E0 kinds, and version 6
admits E1; all five
table-rebuilding upgrades require a verified
whole-file backup before table replacement.

For the intended composed deployment, configure `ArtifactStore` with
`StoreConfig::with_database_path` and pass that same SQLite path to `SqliteJournal`,
`ProjectionStore`, `MigrationEngine`, and `EvidenceStore`. This shared file is what lets a journal
append and its artifact-reference rows commit in one SQLite transaction. If no database path is
selected, the artifact store uses `metadata.sqlite3` under its root; that standalone default is
useful in tests but does not by itself compose an authoritative journal.

## Journal and commit boundary

`AppendRequest::plan` is the effect-free boundary before SQLite. A request binds a `StoreId`, one
`CommandId` and request digest, canonical head expectations, one or more exact complete B3 event
frames, state installs, finalized artifact dependencies, optional authority/registry expectations,
outbox drafts, and optional exact claimed-outbox acknowledgements. Planning applies the configured
collection bounds, checks canonical ordering and unique identities, verifies aggregate sequence
and predecessor continuity, binds every acknowledgement identity and fence into the command
request digest, computes event hashes, and computes the batch hash. Empty batches and noncanonical
or inconsistent requests do not reach the database.

`SqliteJournal::append` starts `BEGIN IMMEDIATE`, then performs these operations as one unit:

1. Resolve the command identity. An exact command/digest match returns the original committed
   result; the same identity with another digest is `IdempotencyConflict`.
2. Compare every aggregate head and optional authority or credential-registry precondition.
3. Require every artifact dependency to be finalized and active in the shared artifact catalog.
4. Insert exact frame bytes and hash-chain fields, state history/current state, artifact references,
   outbox rows, exact claimed-outbox acknowledgements, and any registry state; then advance
   aggregate heads.
5. Record the command's global-position range and batch hash and commit.
6. Re-read the command result before constructing `CommittedBatch`.

File-backed journals enable WAL, `synchronous=FULL`, foreign keys, defensive mode, an explicit busy
timeout, a 32 MiB SQLite value limit, no attached databases, and untrusted-schema handling. One
`SqliteJournal` owns one writable connection and mutating methods require `&mut self`.

`CommittedBatch` and the typed B0/B1 committed observations have private construction and are not
`Clone` or `Copy`. They show that this process observed an exact committed result; they are not
signatures and do not authorize an external effect.

### Idempotency and lost acknowledgements

Callers must retain the original `CommandId` and request digest until the outcome is known. On
`JournalErrorKind::IndeterminateCommit`, call
`SqliteJournal::resolve_command(command_id, request_digest)`:

- `CommandResolution::Committed` is the exact original result.
- `CommandResolution::DefinitelyAbsent` permits the caller to retry the same command identity and
  digest.
- `CommandResolution::Conflict` is terminal for that request and exposes the digest already bound
  to the identity.

Do not generate a new command identity merely because acknowledgement was lost. That would change
an indeterminate retry into a distinct logical transition.

### Sequencing, integrity, and outbox

Global positions are contiguous and one-based. Each aggregate begins at sequence one with the zero
hash and no predecessor event, then advances by one. An event hash binds its aggregate identity,
sequence, predecessor, command, exact frame digest, revision digest, and causal identities. The
batch hash also binds the command request and canonical artifact dependencies.

`SqliteJournal::integrity_scan` validates exact frames, event hashes, aggregate chains and heads,
command ranges and batch hashes, state-current/history agreement, registry rows, and artifact
reference metadata. `integrity_export` performs the same complete scan in one read transaction and
returns the checked records, heads, artifact references, and journal-head digest used by replay and
evidence. Hash chains detect tampering; they do not authenticate the host.

Outbox rows commit with their producing events. `claim_outbox` accepts caller-observed positive
monotonic ticks, advances attempts and a fence, and can reclaim an expired lease.
`acknowledge_outbox` is idempotent for an already acknowledged row and otherwise requires the exact
claim fence. A domain transition that must settle a delivered directive atomically uses
`AppendRequest::with_outbox_acknowledgements`; C0 verifies the same exact claimed fence and applies
the acknowledgement in the event/state compare-and-swap transaction. The outbox contains transport
bytes and destinations; it does not create or reinterpret domain events.

## B0 and B1 durable adapters

The logical values from B0 and B1 are deliberately insufficient to claim durability. C0 binds them
to a journal transaction and returns a move-only post-commit observation.

### B0 kernel transitions

`KernelCommitRequest::genesis` and `KernelCommitRequest::transition` consume the B0 result and bind
it to the exact envelope, command where applicable, retained input references, canonical
`KernelEventDto` frame, revision, and successor-state digest. `commit_kernel_transition` installs a
replay capsule with the event and returns `CommittedKernelTransition` only after the journal commit
and exact event observation succeed.

`SqliteJournal::recover_kernel` does not trust a stored snapshot as a replacement for replay. It
loads the aggregate from genesis, asks the caller's `KernelReplayDriver` to invoke the verified B0
reducers with each exact stored capsule, and compares each emitted frame, sequence, identity, and
successor-state digest. A mismatch is terminal journal corruption.

### B1 commit-once state

The journal currently exposes typed durable adapters for:

- capability use through `CapabilityCommitRequest` and `commit_capability_use`;
- budget changes through `BudgetCommitRequest` and `commit_budget_transition`;
- lease compare-and-swap through `LeaseCommitRequest` and `commit_lease_transition`;
- approval state through `ApprovalCommitRequest` and `commit_approval_transition`;
- credential-registry installation through `CredentialRegistryInstall` in the append transaction;
- authority epochs through `allocate_authority_epoch`.

Each adapter stores a canonical successor value under an exact state revision and returns that
successor only with the corresponding committed batch. Lease and state revisions advance by
checked compare-and-swap. Approval resolutions bind the exact current credential-registry revision,
and snapshot digest; the append precondition and committed receipt also retain the durable
registry row's global generation. That global generation is not the signer credential's generation:
the latter remains part of the already-verified approval resolution and may legitimately differ.
Registry revisions and authority epochs are positive and monotonic; overflow does not wrap.

`BudgetOperation::CancelHeld` has an additional safety gate. The caller must first obtain an opaque
`NonActivationObservation` from `observe_budget_non_activation`. It proves that the current durable
reservation lineage is still the committed `Begin` state. `BudgetCommitRequest::new` binds that
observation to the expected state revision and digest; stale or unrelated observations cannot
authorize cancellation.

## Projection replay and rebuild

`peritus-projection` provides pure versioned folds for lifecycle, budget, authority,
journal-catalog, actual artifact references, and evidence references. A fold receives only a
`FoldContext` containing an integrity-checked committed record and registered frame metadata. It
does not receive a database, filesystem, network, process, clock, or other effect handle.

`replay_from_genesis` checks the export range, global ordering, aggregate sequence and predecessors,
constant revision binding within each aggregate, registered family/schema, typed B3 decoding, fold
invariants, and deterministic encoded state. The artifact-reference projection additionally folds
the actual batch dependencies from `IntegrityExport`, not claims reconstructed from event payloads.

A `Checkpoint` binds projection name and version, schema digest, last journal position,
journal-head digest, and payload digest. The replay result separately records an invariant digest
and record count.

Durable rebuild is a two-stage operation:

1. `rebuild_from_genesis` produces and verifies a complete `RebuildCandidate` in memory.
2. `ProjectionStore::install_shadow` inserts the new generation and advances the active pointer in
   one immediate transaction under an explicit expected-generation compare-and-swap.

The previous active generation remains selected if replay or installation fails. Rebuilding the
same journal/schema binding must produce the same payload and invariant digests; a different result
is a fold-invariant failure rather than a silent replacement.

At startup, `ProjectionStore::plan_startup` returns either `RepairAction::Reuse` or
`RepairAction::RebuildFromGenesis` with `Missing`, `SchemaChanged`, `PositionChanged`,
`JournalHeadChanged`, or `PayloadCorrupt`. There is currently no incremental catch-up or automatic
startup loop: the embedding application must rebuild and install each stale built-in projection.
Old generations are retained by the current implementation; C0 exposes no projection-generation
garbage collector yet.

## Artifact lifecycle and garbage collection

`ArtifactStore::open` canonicalizes the configured root, initializes the fixed layout and catalog,
and runs restart recovery. All object and quarantine paths are derived from validated SHA-256
digests. Callers never provide an internal object path.

The write lifecycle is:

1. `begin_write` validates declared size/limit/configuration and reserves against durable logical
   quota. It creates an exclusive temporary file.
2. `ArtifactWriter::write_chunk` performs checked byte accounting and either writes a whole chunk
   or rejects it. A failed writer cannot be finalized.
3. `finalize` checks exact size and SHA-256, flushes and syncs the temporary file, and publishes by
   an atomic no-replace hard link into `objects/sha256/<prefix>/<digest>`.
4. The object and temporary directories are synchronized and finalized metadata is committed.
   Existing identical content is idempotent; an existing size/digest mismatch is terminal
   corruption.

Publication necessarily precedes catalog insertion. A crash in that interval leaves a verified but
untracked object, never an authoritative reference. On open, recovery removes abandoned temporary
files, re-hashes every cataloged or discovered object, completes interrupted quarantine moves,
moves untracked active objects to quarantine for one recovery cycle, and removes untracked files
already left in quarantine by an earlier completed sweep. Missing cataloged bytes or a path/content
digest mismatch is a terminal integrity failure.

Journal and evidence roots live in the shared `artifact_references` table. A reference may name
only finalized active metadata. `plan_gc` loads the durable inventory and both root sets and creates
a canonical plan for an explicit positive `CollectionGeneration`:

- an active unmarked object is quarantined;
- a marked quarantined object is restored;
- an unmarked object quarantined in a strictly earlier generation is deleted;
- an object quarantined in the current generation is not deleted.

`apply_gc_plan` applies those actions in digest order with durable state changes that
`ArtifactStore::open` can reconcile after interruption. Always compute a fresh plan from current
roots. Never edit object, quarantine, metadata, or reference rows by hand to force collection.

Quota is logical catalog usage, not a substitute for filesystem-capacity monitoring.
`observe_space` reports host capacity without deciding admission; `plan_quota` performs checked
logical accounting.

## Migrations, backup, and restore

`MigrationRegistry` is a statically compiled, contiguous, forward-only registry. Validation
recomputes the SHA-256 of each exact SQL source and rejects transaction control, database
attachment, detachment, and vacuum statements because `MigrationEngine` owns the exclusive
transaction.

`MigrationEngine::open` requires an existing regular SQLite file, canonicalizes database and
backup paths, acquires an exclusive owner lock, enables `synchronous=FULL` and foreign keys, runs
`PRAGMA integrity_check`, and installs only migration-owned catalog tables. `preflight` then checks
registry/history digests, current and target versions, application compatibility, forward-only
ordering, and checked database/backup free-space requirements.

The production registry currently contains exactly one required-backup migration: version 1 for
release `0.0.0`, whose SQL is `PRAGMA user_version = 1;` and whose declared scratch requirement is
64 KiB. The C0 component schemas are still installed by their owning `open` methods. Therefore,
observing migration version 1 is not evidence that journal, artifact, projection, or evidence tables
have all been initialized.

For a risky plan, `apply` records the operation identity, creates a consistent SQLite backup in an
exclusive `.partial` file, syncs it, records its digest and recovery state, atomically renames it to
`migration-<operation>-from-<version>.sqlite3`, verifies it again, and applies all selected SQL in
one exclusive transaction. Applied version, source digest, release, and operation identity are
recorded with the migration. Migrations never execute reverse SQL.

At restart, call `MigrationEngine::reconcile` before opening ordinary C0 adapters. It reports, but
does not silently execute, the required action:

- `ResumeBackup` or `ResumeApply`: rerun `apply` with the same preflight plan and operation identity;
- `RetryApply`: explicitly retry the rolled-back no-backup operation;
- `ReconciledApplied`: the database commit had completed and reconciliation marked it applied;
- `RestoreBackup`: stop and call `restore_backup` for that operation, or follow a separately
  reviewed forward-repair procedure.

`restore_backup` verifies the durable backup digest, uses SQLite's restore API, runs an integrity
check, verifies the restored schema version, and records `Restored`. Restoration is an explicit
operator decision. Preserve the database, WAL/SHM state where applicable, artifact root, backup,
logs, and operation identity before attempting manual repair.

## Evidence provenance and freshness

`EvidenceStore::open` requires the journal event/command and artifact record/reference tables to
already exist in the same SQLite database. Admission verifies every artifact through
`ArtifactStore::verify`, then starts one immediate transaction and compares the draft with both a
fresh durable journal observation and the supplied `IntegrityExport`.

An admitted `EvidenceRecord` binds:

- evidence identity, stable kind and source, and payload digest;
- the exact seven-field `RevisionTuple`;
- global position, event identity/hash, batch hash, and integrity-export journal-head digest;
- B3 family/schema, exact frame digest, and derived family-schema digest;
- the journal batch's actual finalized artifact dependency set;
- canonical direct causal parents, each at an older journal position.

The record, causal links, and evidence artifact roots commit together. Exact retries are
idempotent. Reusing an evidence identity or canonical record digest for different content is a
conflict.

Freshness never rewrites or deletes the record. `evaluate_freshness` and `EvidenceStore::freshness`
return `Current` only when all seven revision components match and no durable invalidation exists.
They identify the first drift in tuple order: acceptance specification, harness, workspace,
workspace generation, workspace revision, policy, then provider profile. A later journal-bound
`EvidenceInvalidation` dominates revision comparison.

`plan_bundle` rejects stale/invalidated records and re-verifies journal frames, causal ancestry, and
artifact bytes. `assemble_bundle` streams deterministic canonical records, exact B3 frames, and
artifacts without buffering complete artifacts. `verify_bundle` accepts only `Read` and rechecks
ordering, bounds, schemas, digests, causality, truncation, trailing bytes, the manifest root, and the
complete bundle digest without consulting live state. Bundles are integrity-verifiable inert bytes;
signing and transport authentication are outside C0.

## Failure and recovery classes

Use the typed kind/code and recovery class, not display text, for automation.

| Area | Recovery class | Operator/caller response |
|---|---|---|
| Journal | `CallerCorrectable` | Correct malformed/noncanonical input and re-plan. |
| Journal | `Reobserve` | Reload heads, state, authority, or registry and produce a new plan. |
| Journal | `ResolveCommand` | Resolve the same command identity and digest before any retry. |
| Journal | `Retry` | Retry after bounded backoff for busy/storage conditions. |
| Journal | `Terminal` | Stop authoritative use; preserve and investigate the store. This includes corruption, unsupported schema, missing required artifact, read-only state, overflow, and idempotency conflict. |
| Projection | `Retry` | Retry a lost active-generation CAS or transient SQLite contention. |
| Projection | `Rebuild` | Discard the candidate/current projection as appropriate and rebuild from a new integrity export. |
| Projection | `RepairJournal` | Stop projection use and repair or restore authoritative journal data; replay cannot invent history. |
| Projection | `CorrectInput` | Correct schema identity, deployment, bounds, or adapter configuration. |
| Artifact | `CorrectRequest` | Fix metadata, limits, expected size/digest, quota request, or GC generation. |
| Artifact | `Retry` | Retry the same safe operation after transient I/O clears. |
| Artifact | `RecoverStore` | Reopen/run recovery and correct permissions/capacity before retrying. |
| Artifact | `TerminalIntegrity` | Stop use and preserve the root/catalog for repair or restore. |
| Migration | `CorrectRequest` | Correct compatibility, target, registry, path, or capacity. |
| Migration | `Retry` | Retry the same safe I/O/preflight step after contention clears. |
| Migration | `Reconcile` | Reopen and inspect the durable operation identity before proceeding. |
| Migration | `RestoreBackup` | Explicitly restore the verified backup. |
| Migration | `Terminal` | Stop; registry drift, integrity failure, or corrupt recovery state needs operator review. |
| Evidence | `CorrectInput` | Correct the record, revision, ancestry, manifest, or bundle. |
| Evidence | `Retry` | Retry after bounded SQLite or I/O contention. |
| Evidence | `RepairDependency` | Repair or restore journal/artifact dependencies before evidence use. |
| Evidence | `RebuildCatalog` | Rebuild the evidence catalog from retained immutable sources. No automatic rebuild API exists yet. |
| Evidence | `ObtainFreshEvidence` | Keep the stale history and obtain a new revision-bound observation. |

Do not treat every `Storage`, `Io`, or `Sqlite` failure as proof of absence. In particular,
indeterminate journal and migration commits have identity-based resolution paths.

## Operator startup and recovery ordering

The following is the safe composition contract for an embedding daemon. It is not yet packaged as
an executable command.

1. Quiesce every writer and preserve the configured database, artifact root, store identity,
   migration operation identities, and backup directory. A fresh deployment must create the empty
   regular SQLite file with deployment-appropriate permissions before `MigrationEngine::open`.
2. Open `MigrationEngine` with the compiled `MigrationRegistry::current`. Call `reconcile` and
   complete every reported action. Preflight and apply the selected target. Drop the migration
   engine before ordinary service ownership begins.
3. Open `SqliteJournal` with the exact durable `StoreId`; verify its hardened settings and obtain an
   `integrity_export`. A store-identity or integrity mismatch blocks startup.
4. Open `ArtifactStore` with the same shared database path. Opening performs restart recovery and
   re-hashes cataloged/discovered files. Treat missing or corrupt referenced content as a startup
   failure. If recovery changed state, obtain a fresh journal integrity export before downstream
   planning.
5. Open `EvidenceStore`, which validates the journal/artifact schema dependencies. Do not admit or
   export evidence until journal and artifact checks have succeeded.
6. For each configured projection, open `ProjectionStore`, call `plan_startup` against the checked
   journal report, and either reuse the exact active generation or rebuild from the same export and
   install it with the observed active-generation CAS.
7. Recover required B0 aggregates with `recover_kernel` and a verified-reducer replay driver.
   Re-observe B1 current state, the authority clock, and the credential registry through C0 APIs;
   do not manufacture committed receipts from decoded rows or DTOs.
8. Resolve every retained indeterminate command identity before accepting a replacement command.
   Then enable command intake. Start outbox delivery only after state recovery; use fresh monotonic
   ticks and exact claim fences.
9. Run GC only as a later explicit maintenance action with a new positive collection generation
   and freshly loaded journal/evidence roots. It is not part of schema or journal repair.

If a terminal integrity error occurs at any stage, keep command intake and effect execution
disabled. Rebuilding projections is safe; rewriting journal rows, inventing artifact metadata, or
deleting evidence is not repair.

## Validation commands

Run these commands from the repository root with the pinned lockfile. The first command validates
all C0 unit and integration targets; the remaining commands enforce formatting, lint, documentation,
formal-verification, architecture, and repository gates.

```sh
cargo test --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-targets --all-features --locked
cargo test --package peritus-conformance --all-targets --all-features --locked
cargo test --doc --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-features --locked
cargo fmt --all --check
cargo clippy --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-features --no-deps --locked
cargo verus verify --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo run --locked --package xtask -- all
just check
just gate-a
```

The C0 crates use real temporary SQLite files and artifact directories. Their focused tests cover
append atomicity and recovery, B0 replay, B1 durable adapters, shadow projection swap/rebuild,
artifact limits/restart/GC, migration backup/reconciliation/restore and the checked-in version-0
fixture, and evidence admission/freshness/bundle verification. Production adapters also run the A2
journal duplicate/restart/stale-CAS suite and replay determinism/restart suite; the
`peritus-conformance` package validates the reusable catalogs themselves. The repository-wide
commands remain necessary because architecture inventories, verification manifests, conformance
catalogs, and generated-policy checks live outside the five crate directories.
