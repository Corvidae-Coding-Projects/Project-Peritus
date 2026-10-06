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
Inline patch construction rejects final content, aggregate content, operation counts, present
preimages, and worst-case recovery manifests that exceed its payload policy before filesystem I/O.
Snapshot restoration uses `SnapshotFile` and `PatchSet::from_snapshot`: it streams exact digest-bound
contents through the same transaction without inheriting inline patch byte or operation ceilings.
Both paths observe complete file contents and verify file identity; observation errors never become
synthetic preimage digests. Version-two snapshot recovery manifests use wide collection counts and
retain the version-one decoder for already prepared inline transactions.

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
