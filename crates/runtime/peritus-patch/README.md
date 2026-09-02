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
Patch construction rejects final content, aggregate content, operation counts, present preimages,
and worst-case recovery manifests that exceed production bounds before filesystem I/O. Files over
the exact-observation limit never receive a synthetic digest that could match a preimage.

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
