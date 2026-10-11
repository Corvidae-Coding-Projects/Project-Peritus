# peritus-evidence

`peritus-evidence` owns immutable evidence records and their durable causal, journal, artifact, and
revision bindings.

## Admission and freshness

The `EvidenceStore` opens a caller-selected SQLite database containing the journal and artifact
schemas. An exact committed draft returns its original receipt before journal-export or artifact
work. This retry receipt describes an immutable past admission; it does not assert current
freshness or current dependency availability.

`EvidenceAdmission` retains the exact draft, immutable checked export, open artifact, hash, and
byte cursor across cancellation or retryable commit contention. Artifact hashing uses bounded
reads outside a writer transaction. Commit compares exact durable journal fields, revision,
causal ordering, actual artifact references, and identity fences in one short transaction.
The checked export's ordered positions and reference intervals support indexed lookups.

Each record binds kind/source, exact seven-field revision, journal position, event and batch
hashes, original export head, B3 schema and frame digest, payload, artifacts, and causal parents.
Revision drift makes present authority stale; explicit invalidation dominates revision comparison.
Neither operation rewrites historical records.

## Portable bundles

`plan_bundle` accepts current authority identities and automatically includes their complete
historical ancestry at its original revisions and digests. Historical records are explicitly
separated from current authority in the manifest. Explicitly invalidated closure is rejected.
All records must remain bound to their original journal frames.

Default bundle policy has no entry-count or byte ceilings. Callers can select optional limits with
`BundleLimits::optional`. Checked arithmetic and the actual input's available bytes enforce
representation safety. Legacy bounded records and all-authority manifests retain their original
v1 encoding and digests; scalable records/manifests use v2, and manifests with historical
provenance use v3. Verification accepts canonical older encodings without rewriting them.

Plans preflight exact serialized sizes and artifact metadata. `BundlePreparation` authenticates
artifact bytes once while writing a private temporary stage. Bounded `advance` calls retain source
hashes and partial staging writes. After completion, `into_export` supplies an owned output cursor
that advances after each accepted write. Cancellation and I/O failures retain that exact cursor.
`assemble_bundle` drains this workflow; `publish_bundle` uses synchronized same-directory,
no-clobber publication and recognizes an existing exact destination.

`BundleVerificationOperation` owns partial fields, artifact hashes, input progress, and canonical
verification. Its bounded steps stop at the verified root trailer, so framed transports need not
close before completion. `into_input` returns the next unread transport byte. `verify_bundle`
adds EOF/trailing-data validation for complete standalone files. An arbitrary blocking `Read` or
`Write` remains the transport owner's responsibility; cancellation is checked between I/O calls.

Bundles provide deterministic integrity verification, not signatures or transport authentication.

## Startup and recovery

`EvidenceStore::open_pending` starts or resumes a durable containment scan. Each
`containment_step` inspects a caller-selected number of identities in WAL read snapshots;
individual corrupt candidates are rechecked and contained in narrow write transactions. A durable
scan generation and identity cursor survive cancellation, contention, and process restart.
`open` drains these same steps for synchronous callers. Reads always validate record integrity,
including before background containment finishes.

Quarantine copies retain every original raw indexed field and byte. Permanent digest identities
support audit lookup even when a raw evidence identity is malformed. `reconcile_quarantined`
checks repaired dependencies against unchanged original evidence. `rebuild_quarantined` accepts
only canonical bytes matching the originally indexed digest, identity, and provenance, and
reconstructs normalized causes and artifact roots atomically. Both revalidate the live journal,
causal parents, and exact artifact bytes. Successful repair retains the original quarantine copy
and its digest-bound resolution; failed repair rolls back. The original quarantine schema migrates
without changing its copied bytes or audit identities.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-evidence --package peritus-debugger
cargo clippy --locked --package peritus-evidence --all-targets --all-features -- -D warnings
```

The public-interface integration cases exercise more than 4,096 causal parents, metadata over
32 MiB, generated cancellation/short-I/O schedules, exact retries with missing dependencies,
SQLite writer contention during artifact hashing, durable scan restart, v1 migration, and
quarantine repair with rollback after dependency corruption.
