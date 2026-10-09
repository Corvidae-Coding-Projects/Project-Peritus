# C1 Git, workspace, and patch boundary

C1 is the production workspace effect boundary for Peritus. It turns already-authorized and
durably observed action plans into explicit Git and filesystem operations while preserving exact
workspace identity, mutation fencing, content history, and restart evidence.

The slice consists of three runtime-layer libraries:

| Crate | Responsibility |
|---|---|
| `peritus-git` | Structured Git repository, object, worktree, status, candidate, snapshot, restore, and reconciliation operations |
| `peritus-patch` | Checked relative paths, typed patch sets, deterministic plans, transactional application, rollback, and interrupted-transaction recovery |
| `peritus-workspace` | Read-only/writable state, exact authority gateway, resource/lease binding, candidate and rollback workflow, artifact manifests, and restart classification |

All three crates are verification class H. Deterministic validation and classification live in
Verus Rust; Git subprocesses and filesystem operations are narrow effect shells. None of the
crates can mark a run accepted, mint a B0/B1/C0 receipt, or merge into a user branch.

## Git boundary

`GitRepository` opens an existing repository through fixed-shape Git commands. It resolves the
repository's canonical worktree/common-directory identity and reported object format, then parses
every commit and tree identifier into an `ObjectId` carrying an explicit SHA-1 or SHA-256 format.
Callers cannot submit raw Git argv.

Read commands disable optional locks. Repository-selection environment variables are cleared so a
model-controlled environment cannot redirect an operation to another repository. User-controlled
path and object values are passed as separate argv entries; shell parsing is never involved.

### Baselines

A `Baseline` contains the exact immutable commit and tree selected when a workspace is provisioned.
Branch names are resolution input only. Later operations retain the resolved object identity and do
not follow a moving branch ref.

### Worktrees

`CreateWorktree` combines a checked `WorktreeName`, destination, immutable baseline, and
`WorktreeAccess`. Writable and read-only worktrees are detached from user branches. Existing
registrations or destinations are conflicts rather than implicit reuse.

Each registration has a bounded, versioned manifest. Normal restart recovery decodes that
manifest, revalidates its repository and worktree binding, and reconstructs a fresh handle instead
of trusting process-local state. If Git created the linked worktree but the caller stopped before
persisting a manifest, `recover_existing_worktree` instead takes the original checked
`CreateWorktree` request and adopts only the exact registered destination at the requested
detached baseline.

Removal is explicit and targets the exact registered worktree. A dirty worktree is not silently
discarded. C1 never rewrites a user branch and never treats the primary checkout as disposable.

### Status and candidate trees

Status uses NUL-delimited porcelain-v2 records. `StatusObservation` preserves tracked, untracked,
ignored, renamed, conflicted, and submodule classifications plus exact HEAD/tree facts and a
canonical digest. Status always inspects submodules and requires detached workspace topology;
repository configuration cannot hide those facts. Malformed or oversized Git output is a protocol
error.

Candidate creation operates only in the isolated writable worktree. It stages the allowed
filesystem result and writes a content-addressed Git tree without advancing a user ref. Any
configured `filter.*.clean`, `filter.*.smudge`, or `filter.*.process` key is rejected before a
filter-capable Git effect; built-in attribute behavior without an external driver remains allowed.
Candidate and snapshot manifests are bounded, versioned, digestible, and decodable after restart;
a retained snapshot manifest can be revalidated into a fresh `CandidateSnapshot` handle.
`CandidateSnapshot` reports the exact workspace and snapshot IDs, commit, tree, retaining ref, and
snapshot-manifest digest. C1-owned refs live below `refs/peritus/workspaces/`.

## Patch boundary

`WorkspacePath` is the only file-target type accepted by a `PatchSet`. It represents one bounded
UTF-8 path relative to a workspace root. Construction rejects absolute paths, traversal, empty
components, alternate separators, control bytes, protected metadata, reserved device names, and
other forms that cannot be interpreted consistently.

Before mutation, resolution checks that the canonical workspace root still matches its opened
identity, rejects symlink components/final targets, and revalidates the containing directory. The
portable C1 checks are supplemented later by C3's native sandbox enforcement.

### Patch model

A patch binds:

- a stable `PatchIdentity`;
- exact `WorkspaceId`, `Generation`, and `RevisionNumber` expectations;
- a bounded canonical list of unique target paths;
- create, replace, or delete intent;
- an absent or exact present `Preimage`;
- final SHA-256 digest and byte length;
- regular or executable `FileMode`; and
- preserve, LF, or CRLF `LineEndingPolicy`.

`PatchSet::new` validates path ordering, conflicts, content and preimage bounds, and the worst-case
recovery-manifest encoding; `PatchSet::plan` then validates the exact current workspace tuple.
Both are independent of filesystem I/O. The returned `PatchPlan` is the only value accepted by the
transaction adapter. Content digests are recalculated from supplied final bytes, and an oversized
observed preimage is never represented by a synthetic matching digest.

### Transaction application

Patch transactions use a protected transaction root separate from the agent-visible workspace.
The root is a dedicated canonical namespace bound to the exact workspace, resource, and
environment; it cannot contain or be contained by the worktree or Git common directory. The
transaction identifier derives from the stable patch identity. Application proceeds through
explicit phases:

1. Re-read and validate every expected preimage.
2. Write every final file under the transaction staging directory.
3. Flush staged files and persist the complete transaction manifest with a canonical checksum over
   every recovery-semantic byte.
4. Move existing targets to transaction backups.
5. Install staged finals or apply deletions.
6. Sync affected directories and re-read every requested result.
7. Mark the transaction installed and remove recovery material only after verification.

An ordinary failure rolls completed operations back in reverse order. If rollback itself cannot be
confirmed, the result is recovery-required rather than success.

### Restart recovery

`recover_transaction` requires an expected `RecoveryBinding` containing workspace, generation, and
revision, then validates the versioned manifest, required backups, and actual targets. It
reports an exact `RecoveryOutcome`: applied, rolled back, dirty, or indeterminate. When a manifest
decodes, `RecoveryOutcome::binding` exposes its observed binding. A binding mismatch is
indeterminate and performs no workspace or transaction mutation. Manifest integrity is verified
before any path or operation field is interpreted. Corrupt metadata is quarantined,
and directories created by an interrupted transaction are removed only when exact rollback can
prove that they are empty. Recovery can be repeated after another interruption.

## Workspace identity and state

`WorkspaceBinding` joins the configured `WorkspaceId`, `ResourceId`, and `EnvironmentId` to one
canonical worktree root and immutable baseline. The registered worktree separately carries its
repository identity. `WorkspaceState` additionally binds the current snapshot, `Generation`,
`RevisionNumber`, prior lease holder, `WorkspaceCondition`, and the consumed-action projection for
the current revision.

The API distinguishes `WritableWorkspace` from `ReadOnlyWorkspace`. A read-only snapshot is fixed
to one immutable snapshot identity and exposes inspection only. It has no patch, candidate, or
rollback method. Writable handles are move-only and owned by one `WorkspaceGateway`.

## Authorization gateway

`WorkspaceGateway` is the only authority-bearing, product-facing mutation surface. The leaf Git
and patch adapters expose checked effects but grant no authority. A caller supplies a
`WorkspaceAuthorizationRequest` borrowing the exact post-commit observations already returned by
C0:

- `CommittedKernelTransition`, containing B0's exact committed successor aggregate/action;
- `CommittedCapabilityUse`, containing B1's exact consumed capability transition; and
- `CommittedLeaseTransition`, containing B1's exact committed active lease-use transition.

The gateway cross-checks the requested action against the supplied committed B0 action phase,
action ID and digest, actor, role, environment, `RevisionTuple`, resource, and capability witness.
It then checks the committed capability transition and lease transition name the same action,
resource, workspace, environment, holder, generation, and current authority observation.

After every comparison succeeds, the writable target exclusively writes and synchronizes a
bounded action-consumption marker before constructing the crate-private, move-only
`MutationPermit`. Each marker binds the workspace, resource, environment, generation, revision,
action ID, and action digest under the separate transaction root. Writable reopen validates and
reloads the current revision's markers, so rebuilding a gateway does not reset receipt consumption.
The permit is consumed inside the requested target operation and is never returned or exported.
Raw B1 values, logical lease claims, uncommitted CAS observations, decoded B3 values, booleans, and
caller-created structs cannot substitute for the exact committed observations.

The gateway checks authority and durably records consumption immediately before the first target
Git/filesystem effect. A workspace generation/revision change or any mismatch among the supplied
committed authority facts rejects the request instead of applying it to a different state.

## Mutation and candidate workflow

The normal writable sequence is:

1. Open a writable workspace from an immutable `Baseline` and exact `WorkspaceBinding`.
2. Obtain current B0/B1 transitions and commit them through C0.
3. Build `WorkspaceAuthorizationRequest` from those exact move-only observations.
4. Submit a checked `PatchSet` to `WorkspaceGateway`.
5. Let the gateway authorize, plan, and transactionally apply the patch; the workspace is now
   `Dirty` while its prior snapshot remains the durable current revision.
6. Obtain separate committed authority for candidate creation, then reconcile the exact
   `MutationOutcome`, write its tree, and retain its C1 snapshot ref.
7. Canonically encode the workspace candidate manifest and finalize it through `ArtifactStore`;
   only then install the successor revision as `Clean`.
8. Pass the move-only `MutationOutcome` or `CandidateOutcome` to later orchestration for the
   matching B0/C0 completion transition.

The finalized workspace manifest directly binds its kind, workspace ID, generation,
previous/successor revision, authorizing action ID and digest, resulting tree, and a subordinate
detail digest. For a candidate, that detail digest combines the installed patch-manifest digest
with the Git candidate-manifest digest; the latter binds the exact status observations. The
move-only `CandidateOutcome` separately exposes the `PatchIdentity`, retained snapshot, canonical
workspace manifest, and finalized artifact identity. Finalization does not make the candidate
accepted.

## Rollback

`RollbackRequest` names a retained snapshot and a successor `SnapshotId`; the authorization
request is supplied separately to the gateway. The gateway rejects another workspace lineage,
restores the snapshot tree, reconciles all files and Git state, advances to a new logical revision,
and finalizes a rollback manifest. Once restoration has changed the worktree, any later failure
leaves the logical condition dirty for explicit reconciliation.

Rollback never changes the immutable baseline and never deletes the abandoned candidate tree,
snapshot ref, or evidence. Content may match an earlier snapshot while the logical revision remains
a new successor.

## Restart reconciliation

`WorkspaceGateway::reconcile_restart` takes the expected B1 `ReconciliationCorrelation`, derives
the observed correlation from target-owned workspace state, recovers every restart-visible patch
transaction with the exact current `RecoveryBinding`, and inspects Git against the baseline and
current snapshot tree. The durable action-consumption ledger is skipped because it is target
metadata rather than a patch transaction. Only exact `txn-` plus 64-lowercase-hex directories are
passed to patch recovery; unrelated namespace entries are hashed as dirty evidence without being
renamed or quarantined. C1 hashes the transaction and Git detail digests into
`ReconciliationEvidence` and classifies the result as:

- `Clean` when correlation is exact, no transaction is unresolved, and Git/filesystem
  state matches the committed snapshot;
- `Dirty` when definite tracked, untracked, ignored, conflict, or interrupted-patch changes exist;
- `Fenced` when the requested correlation differs in workspace scope, fenced generation, or prior
  holder;
  or
- `Indeterminate` when required Git/filesystem facts cannot be established.

C1 does not claim that a prior process holder is quiescent. C2 supplies that independent evidence
before B1 can combine a `Clean` resource observation into `SafeToAcquire`.

## Stable failure handling

Each crate exposes an error code/kind, operation, recovery class, and bounded diagnostic context.
Common recovery classes distinguish caller correction, retry, reopen/reconcile, rollback/recover,
manual inspection, and terminal corruption. Expected malformed input and normal Git/filesystem
failures return errors rather than panicking.

Important failure outcomes include:

- repository/object/worktree mismatch;
- malformed Git protocol output;
- dirty or conflicted worktree;
- invalid, protected, or symlink path;
- stale workspace generation/revision;
- preimage or final digest mismatch;
- patch rollback failure or interrupted transaction;
- B0/B1/C0 authorization mismatch;
- stale or fenced lease; and
- dirty or indeterminate restart state.

## Refinement evidence

C1 owns three architecture refinements:

- `REF-C1-B1-RESOURCE-IDENTITY`: resolved targets, capability permissions, lease scope, B0 witness,
  and workspace binding name the exact same `ResourceId`.
- `REF-C1-B1-RECONCILE-SAFETY`: only an exactly correlated and complete clean observation can
  produce the resource-safety half of `SafeToAcquire`; dirty/unknown observations stay non-safe.
- `REF-C1-B1-AUTHORITY-GATE`: only the target-owned gateway can construct and consume its private
  mutation permit after exact current committed observations match.

The corresponding named Verus rules and executable refinement tests are registered in
`verification/obligations.toml` before the reservations are removed from `architecture.toml`.

## Verification

Focused development checks are:

```text
cargo test --package peritus-git --package peritus-patch --package peritus-workspace --all-targets --all-features --locked
cargo clippy --package peritus-git --package peritus-patch --package peritus-workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-git --package peritus-patch --package peritus-workspace --all-features --no-deps --locked
cargo verus verify --package peritus-git --package peritus-patch --package peritus-workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo test --package peritus-conformance --all-targets --all-features --locked
just gate-a
```

Integration tests use real temporary repositories, detached worktrees, files, symlinks where the
platform supports them, Git candidate objects, artifact directories, C0 stores, deterministic A2
identities, and named fault points. No mock is accepted as evidence for Git/file effects or durable
authorization receipts.
