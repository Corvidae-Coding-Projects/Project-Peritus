# Feature: C0 journal, projections, artifacts, migrations, and evidence

## Summary

C0 supplies the durable state boundary for Project Peritus. It adds five state-layer crates:
`peritus-journal`, `peritus-projection`, `peritus-artifact-store`, `peritus-migrations`, and
`peritus-evidence`. Together they persist the exact B3 bytes selected by verified B0/B1/B2 logic,
enforce compare-and-append preconditions in one SQLite transaction, rebuild deterministic query
state without effects, finalize content-addressed artifacts safely, evolve the stored schema, and
produce revision-bound evidence views.

This is the production persistence contract, not an interim store. C0 does not execute workspace,
process, tool, provider, or orchestration effects. It records already-planned transitions and
returns observations that higher layers must match against the exact request they retained.

## User-visible behavior

1. Accepted commands survive process termination and host restart once C0 reports a successful
   commit.
2. Retrying one command identity either returns the original exact committed result or a typed
   conflict; it never appends the same logical transition twice.
3. State can be rebuilt from genesis and checked against persisted projection checkpoints without
   executing external effects.
4. Corrupt, missing, reordered, or truncated journal records are detected before the affected
   state is treated as current.
5. Artifact writers stream bounded data, verify its digest and size, and atomically publish it.
   Partial files are never authoritative evidence.
6. Database upgrades are ordered, preflighted, backed up when risk requires it, and leave recovery
   metadata sufficient to restore the compatible pre-migration state.
7. Evidence consumers receive only digest-verified records bound to one exact `RevisionTuple`,
   journal position, causal lineage, and artifact set.

## Requirements

### R-C0-001 — transactional exact-byte journal

`peritus-journal` stores complete B3 frames without reserialization. One SQLite transaction checks
the aggregate head, command idempotency identity, current authority epoch or registry revision when
present, inserts the event batch, advances affected heads, installs durable state records, and
creates outbox rows. SQLite runs in WAL mode, authoritative commits use `synchronous=FULL`, foreign
keys and defensive limits are enabled, and one `SqliteJournal` value owns the writable connection.

### R-C0-002 — deterministic append plan

Effect-free code validates an `AppendRequest` before I/O. A request names its store identity,
command identity and digest, one or more aggregate preconditions, canonical event frames, exact
causal predecessor, state-install records, artifact dependencies, authority epoch/registry
preconditions, and outbox entries. It computes the event and batch hashes with checked arithmetic
and rejects duplicate identities, empty batches, inconsistent sequences, missing artifact
dependencies, or noncanonical ordering before opening a transaction.

### R-C0-003 — sequencing and hash chains

Every aggregate starts at sequence one and advances once per stored event. Each event hash binds a
domain-separated version tag, aggregate key and kind, sequence, event identity, previous event
identity, previous event hash, command identity, canonical frame digest, revision digest, and causal
identities. Genesis uses the declared zero predecessor. Integrity scan recomputes hashes over the
stored bytes and checks aggregate heads against the final records.

### R-C0-004 — idempotency and indeterminate resolution

The command table binds an idempotency key to request digest and result event range. Exact replay
returns an observation of the original range. Reuse with a different digest is a conflict. A caller
that loses acknowledgement resolves the same command identity; it must not submit a new identity
until C0 reports committed or definitely absent.

### R-C0-005 — opaque commit receipts

`CommittedBatch`, `CommittedKernelTransition`, `CommittedBudgetTransition`,
`CommittedLeaseTransition`, and `CurrentCredentialRegistry` have private fields, no public
constructors, no decode implementation, and are not `Clone` or `Copy`. Only a successful committed
transaction plus exact post-commit observation constructs them. They expose bounded identity,
position, hash, and revision accessors needed by later gateways but are not effect permits.

### R-C0-006 — B0 durable transition

The B0 adapter consumes an accepted `KernelTransition`, persists the exact `KernelEventDto` frame
and replay capsule, compares the previous event identity/sequence/revision, and returns the next
`KernelAggregate` only inside `CommittedKernelTransition` after commit. Recovery reconstructs B0
state by replaying stored command/input capsules through B0 reducers and comparing every emitted
event frame and successor digest. A stored snapshot is only a checkpoint optimization and never a
replacement for verified replay.

### R-C0-007 — B1 commit-once transitions

Typed adapters persist capability use, budget transition, lease CAS, approval transition, and
credential-registry changes in the same journal transaction as their corresponding event/state
records. Budget `CancelHeld` additionally requires a C0-owned durable observation that the matching
committed begin lineage never reached activation. A raw B1 receipt, reservation reference,
transition, lease CAS claim, approval value, or registry snapshot cannot construct a C0 receipt.

### R-C0-008 — authority clock and credential registry

Authority epoch allocation is one atomic compare-and-swap operation. Epochs and credential
generations/revisions are positive, strictly increasing, never reused, and overflow with a typed
terminal error. Credential issue, disable, revoke, and same-key reissue atomically install the exact
checked snapshot bytes and advance revision/generation. `CurrentCredentialRegistry` binds the exact
stored snapshot digest and revision observed after commit.

### R-C0-009 — transactional outbox

Outbox rows are inserted with their producing events. Claims use bounded attempts and lease/fence
metadata; acknowledgement is idempotent. The outbox transports bytes and destinations only. It
cannot create or reinterpret domain events.

### R-C0-010 — deterministic projections

`peritus-projection` defines versioned pure fold contracts and concrete lifecycle, budget,
authority, journal-catalog, artifact-reference, and evidence projections. A projection consumes
checked committed records in global-position order and cannot perform I/O. Checkpoints bind name,
version, last global position, journal head digest, projection payload digest, and schema digest.

### R-C0-011 — replay and rebuild

Replay from genesis rejects gaps, duplicates, order changes, unsupported record families, invalid
frames, stale revisions, and fold invariant failures. Rebuild writes a shadow projection, verifies
its checksum/invariants against the journal head, then atomically swaps its catalog pointer.
Startup compares all checkpoints with journal heads and repairs or rebuilds stale projections.
State and decision replay never execute external effects; simulation/live reproduction remain later
integration modes.

### R-C0-012 — artifact streaming and finalization

`peritus-artifact-store` streams to an exclusive temporary file under the store, enforces declared
and configured byte limits, hashes while writing, flushes and syncs the file, checks expected
size/digest, creates fan-out directories, atomically publishes to
`objects/sha256/<prefix>/<digest>`, and syncs the parent directory. Existing identical content is
idempotent. Existing mismatched content is corruption.

### R-C0-013 — artifact metadata, references, quotas, and collection

SQLite metadata tracks digest, size, media type, encryption metadata, finalization state, creating
event, and quarantine state. A transaction may reference only a finalized digest. Verified planning
enforces quota with checked arithmetic. Garbage collection is deterministic mark-and-sweep from
journal/evidence roots, first moves unreferenced objects to quarantine, and deletes them only on a
later explicitly applied plan. Recovery removes or quarantines abandoned temporary files.

### R-C0-014 — forward-only migrations

`peritus-migrations` owns an ordered registry whose canonical SQL/source bytes have stable SHA-256
digests. Preflight validates current/target versions, registry contiguity, database integrity,
required free space, backup policy, and application compatibility. Risky migrations use SQLite's
consistent backup support before applying. Applied version/digest/release and recovery metadata are
committed transactionally. Old-schema fixtures migrate to the current version and verify replay.

### R-C0-015 — evidence provenance and freshness

`peritus-evidence` defines immutable evidence records, manifests, invalidations, causal links, and
bundle plans. Admission requires a matching digest-verified committed journal record, exact
`RevisionTuple`, existing finalized artifacts, and valid causal ancestry. A later revision or
explicit invalidation makes the observation stale without deleting its history. Deterministic
bundle assembly sorts entries canonically and binds manifest, record, artifact, and journal hashes.

### R-C0-016 — bounded failures and diagnostics

All libraries expose stable error kinds/codes and recovery classes. Expected failures include
stale CAS, idempotency conflict, indeterminate commit, corrupt chain, unsupported schema, busy or
read-only store, quota exhaustion, partial artifact, missing artifact, insufficient migration
space, failed backup, and projection mismatch. Recoverable environment errors preserve their
source. No public operation panics for malformed data or normal I/O failure.

### R-C0-017 — verification and trust boundary

Sequence, hash-input construction, append-plan validation, idempotency decisions, authority epoch
allocation, projection fold/checkpoint rules, GC planning, migration planning, and evidence
freshness/bundle planning are executable Verus code where supported. SQLite, filesystem sync/rename,
free-space observation, and OS errors are narrow audited H/T adapters. No adapter decides that a
run is accepted, an approval is current, a transition committed, or an artifact is valid without
the checked result path.

## Acceptance criteria

1. All five crates exist under `crates/state/`, remain under architecture size limits, have crate
   documentation, typed public errors, focused unit/integration tests, and no reachable placeholder
   success paths.
2. SQLite connection tests prove WAL, `synchronous=FULL`, foreign keys, busy timeout, schema
   installation, and single-transaction append/head/idempotency/outbox behavior.
3. Journal tests cover multi-event atomicity, stale heads, sequence gaps, duplicate command replay,
   conflicting command reuse, failure before commit, lost acknowledgement resolved after commit,
   corrupted payload/hash/head data, restart, and integrity export.
4. Typed integration tests prove B0 next state is unavailable before commit, exact after commit,
   and reconstructible after restart; B1 logical receipts or CAS observations cannot satisfy the C0
   receipt APIs.
5. Budget tests cover begin/activate/usage/finalization and prove `CancelHeld` requires the matching
   committed begin plus C0 non-activation observation.
6. Credential and epoch tests cover issue/disable/revoke/reissue, stale snapshot rejection,
   monotonic allocation, restart, and overflow.
7. Projection tests rebuild from genesis, compare checkpoints, repair stale state, reject corrupt or
   unsupported records, and demonstrate that folds cannot invoke effects.
8. Artifact tests cover chunked streaming, exact limit, one-over, digest/size mismatch, duplicate
   identical content, collision/corruption handling, crash before/after rename, restart cleanup,
   quotas, reference marking, quarantine, and sweep application.
9. Migration tests run every checked-in historical database fixture through preflight, backup,
   migration, integrity scan, replay, and rollback restoration.
10. Evidence tests cover valid admission, each `RevisionTuple` field drifting independently,
    missing/corrupt artifacts, invalid causal links, invalidation, canonical bundle determinism, and
    bundle digest verification.
11. A2 journal and replay suites contain real cases and pass against the SQLite implementation;
    they are no longer empty catalog placeholders.
12. Architecture, controlled generated/fixture roots, Cargo lockfile, dependency policy,
    verification manifests/exclusions, strict package inventories, CI, and documentation register
    every new crate.
13. `REF-C0-B0-DURABLE-TRANSITION`, `REF-C0-B1-COMMIT-ONCE`,
    `REF-C0-B1-CREDENTIAL-REGISTRY-CURRENT`, `REF-C0-B1-CLOCK-EPOCH`, and
    `REF-C0-B2-EVIDENCE-PROVENANCE` are removed from reservations only after named source symbols,
    tests, and manifest evidence exist.
14. Focused tests, generated fixtures, `cargo fmt`, strict Clippy/rustdoc, ordinary API checks,
    architecture checks, full Verus/no-cheating verification, `just check`, and `just gate-a` pass.
15. The reviewed change is committed, pushed, merged through the protected pull-request path, and
    exact `main` is verified.

## Current architecture

B0 owns a verified value-in/value-out lifecycle reducer whose successful `KernelTransition`
contains one next `KernelAggregate` and one `KernelEvent`; it is explicitly not a durable receipt.
B1 similarly exposes logical capability, budget, lease, approval, and credential values, including
a `LeaseCasPort` reserved for C0. B2 owns checked immutable contracts and revision-bound acceptance
evidence. B3 freezes bounded canonical command/event/contract frames and deliberately decodes them
as inert data. A2 supplies deterministic IDs, clocks, faults, and currently empty journal/replay
conformance catalogs.

There is no database, artifact store, migration registry, projection implementation, durable epoch
allocator, current credential-registry observation, or authoritative evidence store. All five C0
refinements remain open in `architecture.toml`.

## Proposed design

### Crate and dependency boundaries

```text
peritus-journal       -> peritus-codec, peritus-protocol, B0/B1 types, rusqlite
peritus-artifact-store-> peritus-types, peritus-codec, sha2, fs4
peritus-projection    -> peritus-journal, peritus-protocol, B0/B1/B2 values
peritus-migrations    -> peritus-journal, peritus-artifact-store, rusqlite, fs4
peritus-evidence      -> peritus-journal, peritus-artifact-store, peritus-projection,
                         peritus-spec, peritus-quality-policy
```

All crates are class H because their deterministic cores are verified and their public ordinary
Rust surfaces call those same bodies after ghost erasure. `rusqlite` and direct filesystem modules
are enumerated effect boundaries; they do not become dependencies of foundation crates.

Pinned dependencies are `rusqlite = 0.40.2` with default features disabled and the `bundled` and
`backup` features enabled, `fs4 = 1.1.0` for cross-platform free-space observations, and
`tempfile = 3.27.0` as a dev dependency. Bundled SQLite makes the exact supported database feature
set reproducible across Linux, macOS, and Windows.

### Journal modules

```text
src/lib.rs
src/error.rs
src/identity.rs
src/record.rs
src/head.rs
src/hash_chain.rs
src/append_plan.rs
src/idempotency.rs
src/outbox.rs
src/receipt.rs
src/integrity.rs
src/authority.rs
src/domain/{mod.rs,kernel.rs,budget.rs,lease.rs,approval.rs}
src/sqlite/{mod.rs,connection.rs,schema.rs,append.rs,query.rs,recovery.rs,lease_cas.rs}
```

The `sqlite` module translates checked plans into parameterized SQL and translates rows into
bounded observations. It never accepts arbitrary table names or SQL fragments. The outer API keeps
one mutable connection owner so concurrent callers serialize at the daemon boundary; tests open
separate connections to exercise real stale-CAS behavior.

### Initial SQLite schema

The initial schema contains `store_meta`, `aggregate_heads`, `events`, `commands`, `state_records`,
`outbox`, `authority_clock`, `credential_registry`, `projection_catalog`, `artifact_records`,
`artifact_references`, `evidence_records`, `evidence_invalidations`, `schema_migrations`, and
`recovery_operations`. IDs and digests use fixed-length BLOB constraints; sequence/position/version
columns use positive integer checks; enumerations use closed integer tags. The event payload column
stores the exact complete B3 frame. A schema fixture and digest are checked in under
`persistence/fixtures/v1/`.

### Transaction boundary

The adapter begins an immediate transaction, resolves the command identity, reads every named
head/currentness precondition, verifies finalized artifact dependencies, inserts events and state
records, advances heads/authority rows, inserts outbox entries, records the command result, and
commits. Any mismatch rolls back without a partial authoritative row. After a successful SQLite
commit, C0 re-reads the command result and head hashes before constructing the move-only receipt.
If commit returns an indeterminate I/O result, C0 performs command-resolution instead of guessing.

Artifacts are prepared and finalized before this transaction. The journal transaction only creates
their durable references. An unreferenced finalized artifact is harmless and later collectible; a
journal row referencing a partial artifact is forbidden.

### Projection modules

```text
src/lib.rs
src/error.rs
src/fold.rs
src/checkpoint.rs
src/catalog.rs
src/replay.rs
src/rebuild.rs
src/lifecycle.rs
src/budget.rs
src/authority.rs
src/artifacts.rs
src/evidence.rs
src/sqlite/{mod.rs,store.rs,swap.rs}
```

`Projection` is a pure trait over checked `CommittedRecord` values. The SQLite adapter feeds it
records and persists its encoded payload/checkpoint; it is not available inside the fold method.
Rebuild uses a new generation row and changes the active catalog pointer only after all records and
invariants pass.

### Artifact-store modules

```text
src/lib.rs
src/error.rs
src/config.rs
src/digest.rs
src/path.rs
src/writer.rs
src/finalize.rs
src/metadata.rs
src/quota.rs
src/references.rs
src/gc_plan.rs
src/recovery.rs
```

The store root is canonicalized once and all internal paths are derived from validated digest
bytes, not user path strings. `ArtifactWriter` owns its temporary file and removes it on clean
abort/drop where possible. Recovery handles leftovers after abnormal termination.

### Migration modules

```text
src/lib.rs
src/error.rs
src/registry.rs
src/descriptor.rs
src/preflight.rs
src/plan.rs
src/backup.rs
src/apply.rs
src/recovery.rs
src/fixtures.rs
```

Migration SQL is compiled into the binary and included in its descriptor digest. Planning is pure;
backup and SQL execution are separate effect steps whose observations are checked before progress
is recorded. Version one is installed through the same registry path as every later version.

### Evidence modules

```text
src/lib.rs
src/error.rs
src/record.rs
src/manifest.rs
src/freshness.rs
src/causality.rs
src/admission.rs
src/invalidation.rs
src/bundle.rs
src/projection.rs
```

Evidence records contain IDs, stable kind/source tags, `RevisionTuple`, producing event and journal
position/hash, payload digest, artifact digests, causal parents, and invalidation state. Portable
bundles contain a canonical manifest plus exact journal frames and artifact bytes or explicit
profile-approved omissions. Signing/export transport is a later application boundary; C0 produces
the deterministic bytes and root digest.

### Alternatives considered

A custom append-only file plus sidecar indexes would make raw appends simple and avoid SQLite FFI,
but C0 would then need to implement atomic multi-aggregate commits, uniqueness, crash-safe indexes,
outbox transactions, migrations, backups, and concurrent readers itself. That increases the trusted
I/O surface and makes recovery harder to test. SQLite is preferred because its transaction and WAL
semantics match the local single-writer daemon architecture.

Storing artifacts as SQLite blobs would simplify references but makes streaming, large-output
retention, backup sizing, and content-addressed deduplication worse. Filesystem objects plus
transactional SQLite metadata preserve a smaller, clearer failure boundary.

## Data and compatibility

The database starts at schema version one. Every migration and fixture is immutable after merge.
Stored B3 frames retain their original bytes and family/schema tags forever. Unknown future frames
remain exportable and integrity-checkable even when a projection cannot interpret them. A semantic
reinterpretation requires a new record or protocol version and a forward migration; history is
never rewritten merely to adopt a new encoder.

Artifact paths are derived exclusively from lowercase SHA-256 hex. Metadata can add fields in later
schema versions without moving content. Projection schemas are independently versioned and always
rebuildable, so they are not authoritative migration inputs.

## Failure handling

- Failure before SQLite commit leaves no authoritative rows.
- An indeterminate commit is resolved by command identity and request digest.
- Failure after artifact rename but before journal reference leaves an unreferenced collectible
  object, not a false evidence claim.
- Failure before artifact rename leaves a temporary file removed during recovery.
- Corrupt event bytes, hashes, heads, or state records quarantine the affected aggregate and return
  a terminal integrity error; repair never invents replacement history.
- Busy/locked databases return a bounded retryable error after the configured timeout.
- Projection failure leaves the old active generation unchanged.
- Migration failure restores from the completed backup or retains explicit recovery-required state;
  a previous binary is never opened against a newer schema.
- Garbage collection never deletes directly from the first mark result; quarantine makes deletion
  a distinct recoverable operation.

## Security considerations

Canonical bounds are enforced before allocation. SQL is static and parameterized. SQLite extension
loading is unavailable. Database and object roots are explicit local paths with private-file
creation expectations; no user-controlled path is joined below the artifact root. Hash chains are
tamper evidence, not host authentication. Artifact digests provide identity, not confidentiality;
encryption metadata is stored now while C3/G0 later supply OS-backed key handling.

Commit receipts are Rust capability values with private construction, not signatures. They prove
that this process received an exact post-commit observation from its journal owner; callers that
restart must re-observe current state through C0. Raw database rows, B3 DTOs, and reconstructed
projection values remain inert until checked by their owning verified reducer/currentness gate.

## Verification

Focused commands begin with:

```text
cargo test --package peritus-journal --package peritus-artifact-store --all-targets --all-features --locked
cargo test --package peritus-projection --package peritus-migrations --package peritus-evidence --all-targets --all-features --locked
cargo clippy --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-features --no-deps --locked
cargo verus verify --package peritus-journal --package peritus-projection --package peritus-artifact-store --package peritus-migrations --package peritus-evidence --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo run --locked --package xtask -- all
just check
just gate-a
```

Tests use real temporary SQLite databases and artifact directories, deterministic A2 identities,
the A2 fault injector at named realistic boundaries, B3 compatibility frames, and checked-in old
schema fixtures. Failpoints are explicit injected returns around transaction, sync, rename, backup,
and projection-swap operations; production code has no ambient random failure mode.

## Rollout and rollback

C0 lands before production data exists. Version-one fixtures are created with the implementation
and become immutable compatibility inputs. Internal rollout uses disposable databases and artifact
roots. Once a later slice persists non-test state, rollback means restoring the tested compatible
database backup and immutable artifact set or applying a forward repair; deleting or rewriting
journal history is not a rollback strategy.

## Open questions

None block implementation. SQLite is the architecture-selected authoritative store; bundled
`rusqlite` provides the reproducible cross-platform adapter. Commit receipts are process-local opaque
capabilities backed by re-observable durable identities, while exported evidence authenticity remains
a later signing boundary.

## Out of scope

- Workspace, Git, patch, process, sandbox, network, tool, or provider execution.
- Daemon supervision, startup fencing orchestration, IPC, or public CLI/TUI commands.
- OpenTelemetry export and trace redaction pipelines.
- OS credential-store integration and encryption key management; C0 stores encryption metadata and
  refuses unavailable encrypted content but does not invent a key provider.
- Live reproduction, effectful simulation, or execution during replay.
- Remote/distributed databases, multi-daemon writers, consensus, or cloud artifact storage.
