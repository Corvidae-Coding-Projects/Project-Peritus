# peritus-patch

`peritus-patch` owns Project Peritus's checked workspace-relative paths, typed create/replace/delete
patches, deterministic patch plans, and recoverable multi-file filesystem transaction adapter.

Planning is independent of the filesystem. Application requires a `PatchPlan`, verifies every
preimage before mutation, stages complete final files below a separate protected transaction root,
persists a versioned recovery manifest, and then installs the canonical operation sequence. A
reported ordinary failure has restored all original files. If restoration cannot be proved, the
transaction remains available to restart recovery and the error is explicitly indeterminate.
The manifest carries a canonical SHA-256 checksum over every recovery-semantic byte; restart
decoding verifies it before interpreting paths, preimages, or operation state.
Patch admission has no per-file, aggregate-byte, operation-count, or manifest-message ceiling.
`FinalFile` accepts owned bytes; `SnapshotFile` retains owned streaming storage and can be used with
ordinary `PatchSet::new` as well as `PatchSet::from_snapshot`. Staging verifies every complete final
before the first target effect. Preimages are fully hashed and originals remain in durable backup
storage until the whole transaction is settled.

Previously accepted inline patches retain their exact identities and schema-one/three receipts;
snapshot identities and schema-two/three receipts also remain unchanged. New ordinary patches that
do not fit the historical inline representation bind workspace/version, ordered paths, operation
kinds, exact preimage/postimage digests, sizes, modes and inline line-ending intent under a separate
version-four identity. Schema-four manifests stream deterministic 64 KiB physical pages with wide
counts, page checksums and a whole-manifest checksum. There is no page-count allowance. A complete
replacement is synchronized before atomic publication of the recovery root; pages never authorize
separate effects. All legacy decoders remain available for restart recovery.

Workspace paths have no component-byte, total-byte, or depth allowance. Authority paths remain
UTF-8, canonical relative paths with no traversal, NUL, or protected metadata. Unix paths accept
native punctuation, controls and trailing characters; Windows keeps alias/device-name safeguards.
Native or extended paths use a platform-bound version-five identity and recovery manifest whose
records can span physical byte pages, without a record-size or total-page allowance. Old identities
and schema-one through schema-four bytes remain exact. Older binaries cannot recover schema-five
transactions; upgrade recovery readers before using newly accepted paths. Physical filesystem
constraints are still reported as original I/O failures.

The historical `MAX_FILE_BYTES`, `MAX_PATCH_BYTES` and `MAX_PATCH_OPERATIONS` exports are retained
for source compatibility and describe the old inline encoding only. They do not reject patches.
Independent caller policy/transport ceilings remain owned by those callers. This API still owns
the complete operation metadata and returns complete receipt bytes in memory; physical memory or
storage failure is not replaced by a synthetic byte/count allowance. All representations observe
complete contents; observation errors never become synthetic preimage digests.

Restart recovery requires a `RecoveryBinding` containing the expected workspace identity,
generation, and revision. A decoded manifest with a different binding produces an indeterminate
`RecoveryOutcome`; `binding()` exposes the observed manifest binding, and the mismatch causes no
workspace or transaction mutation. Recovery also tracks directories that were absent before the
transaction. It removes them only when rollback can do so exactly; a nonempty directory makes the
result indeterminate rather than silently claiming restoration.

This crate grants no workspace authority. `peritus-workspace` owns the B0/B1/C0 authorization
gateway and is the product-facing mutation surface.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-patch
```
