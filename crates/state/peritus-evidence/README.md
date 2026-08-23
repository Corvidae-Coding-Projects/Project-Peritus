# peritus-evidence

`peritus-evidence` owns immutable evidence records and their durable causal, journal, artifact, and
revision bindings.

## Admission and freshness

The `EvidenceStore` opens a caller-selected SQLite database that already contains the
`peritus-journal` and `peritus-artifact-store` schemas. Admission uses one `BEGIN IMMEDIATE`
transaction to compare an integrity-checked journal export with the exact durable event and command
rows, validate the complete `RevisionTuple`, prove direct parents are older, require the journal's
actual artifact dependency set, and insert the evidence record plus durable artifact roots. Exact
retries are idempotent; identity reuse with different content is rejected.

Each immutable record binds its payload, kind/source, exact seven-field revision, journal position,
event and batch hashes, integrity-export head, B3 family/schema and exact frame digest, actual
artifact set, and canonical causal parents. Admission re-hashes every artifact through the artifact
store. A later journal-bound invalidation or drift in any revision component makes a record stale
without deleting its history; explicit invalidation dominates revision comparison.

## Portable bundles

Portable bundles are canonical and deterministic. Planning rejects stale or invalidated evidence,
missing causal ancestry, changed journal frames, and changed artifacts. Assembly streams artifact
objects rather than buffering them. `verify_bundle` is intentionally effect-free: it accepts only a
`Read` stream and rechecks the manifest, records, B3 frames and schemas, artifact bytes, canonical
order, causal order, root digest, total digest, bounds, truncation, and trailing data.

The deterministic revision, causal-position, and bundle-order predicates are executable Verus
functions used by the ordinary Rust admission, freshness, planning, and verification paths.

Bundles provide deterministic integrity verification, not signatures or transport authentication.
The current crate does not automatically rebuild a corrupt evidence catalog; callers must retain
immutable source material and stop evidence use when a dependency or catalog integrity error is
reported.

See [C0 durable state](../../../docs/c0-durable-state.md) for provenance details, failure recovery,
startup ordering, and exact validation commands.
