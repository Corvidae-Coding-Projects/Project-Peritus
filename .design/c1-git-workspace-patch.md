# Feature: C1 Git, workspace, and atomic patching

## Summary

C1 supplies the explicit workspace boundary for Project Peritus. It adds three class-H runtime
crates: `peritus-git`, `peritus-patch`, and `peritus-workspace`. Together they create isolated Git
worktrees from immutable baselines, represent workspace-relative paths and patches as checked
values, apply multi-file changes transactionally, reconcile the filesystem with Git, create
content-addressed candidate snapshots, and restore earlier snapshots without erasing history.

C1 is an effect boundary, not an authority source. Deterministic path, patch, state, authorization,
and reconciliation decisions are executable Verus Rust. Git subprocesses and filesystem calls are
narrow ordinary-Rust shells that accept checked plans. A workspace mutation is reachable only
through `peritus-workspace`, which cross-checks the current workspace, B0 action, B1 capability and
lease, and C0 post-commit observations before constructing its crate-private mutation permit.

The implementation favors the direct developer loop needed by the product: real repositories,
real worktrees, real files, exact status, patch, snapshot, rollback, and restart behavior. Native
OS sandboxing and process supervision remain C2/C3 responsibilities.

## User-visible behavior

1. A writable run workspace is a dedicated detached Git worktree anchored to an exact baseline
   object ID. User branches and unrelated dirty state are never rewritten.
2. Reviewers and other read-only consumers receive a separate worktree fixed to one immutable
   snapshot, not a shared writable directory.
3. A patch names the expected workspace generation and revision, every target, its expected
   preimage, intended final contents or deletion, file mode, and line-ending policy.
4. A valid multi-file patch either leaves every requested result installed or restores every
   original file and reports a typed failure. Restart reconciliation detects interrupted patch
   transactions rather than assuming success.
5. Candidate creation reconciles Git and filesystem state, records exact tracked, untracked, and
   ignored observations, writes a content-addressed Git tree, and advances the logical workspace
   revision only for the observed result.
6. Rollback materializes a known snapshot as a new logical revision. The abandoned candidate and
   its evidence remain addressable.
7. Stale leases, stale generations, resource mismatches, unauthorized actions, protected metadata,
   traversal, symlink targets, and preimage mismatches fail before mutation.

## Requirements

### R-C1-001 — real Git repository boundary

`peritus-git` opens only an existing Git worktree or repository, discovers the common directory and
object format through structured `git` commands, and validates all returned object IDs. It invokes
Git directly with argv, a fixed working directory, cleared repository-selection environment
variables, `--no-optional-locks` for reads, and no shell interpolation. Errors retain the command
operation, exit status, and bounded stderr.

### R-C1-002 — immutable baseline and worktree lifecycle

A baseline resolves once to an immutable commit and tree object ID. Worktree creation uses an exact
detached baseline, a validated worktree name, and an explicit destination outside the protected
repository metadata. Existing destinations or registrations are conflicts. Removal targets only
the exact registered worktree and refuses a dirty worktree unless the caller supplies an explicit
already-authorized force plan.

### R-C1-003 — exact status and reconciliation

Git status is parsed from NUL-delimited porcelain-v2 output into bounded typed entries. Tracked,
untracked, ignored, renamed, conflicted, submodule, and metadata changes remain distinguishable.
The observation binds repository identity, worktree root, HEAD, index tree when available, and a
canonical status digest. Malformed or oversized output is a typed protocol failure.

### R-C1-004 — content-addressed snapshots and candidates

Candidate creation stages the complete allowed workspace result in the isolated worktree, writes a
Git tree, and returns its validated object ID plus a canonical manifest/digest. It does not advance
a user branch. Read-only snapshots are detached worktrees at an immutable snapshot commit/tree.
Snapshot objects are retained through a C1-owned namespaced reference until their lifecycle owner
explicitly releases them.

### R-C1-005 — rollback preserves history

Rollback verifies the requested snapshot belongs to the same workspace lineage, restores its tree
into the writable worktree, reconciles the result, and returns a new successor revision outcome.
It does not delete the abandoned tree, rewrite the baseline, reset a user branch, or claim that the
restored content is the earlier logical revision.

### R-C1-006 — checked workspace paths

`peritus-patch::WorkspacePath` owns a bounded UTF-8 path in one canonical slash-separated form.
It rejects empty/absolute paths, empty components, `.` and `..`, NUL/control characters, platform
prefixes, alternate separators, trailing separators, overlong components/paths, reserved device
names, and protected top-level metadata. Resolution starts from an already-opened canonical
workspace root, rejects symlink components and symlink final targets for mutation, and rechecks the
resolved parent immediately before replacement.

### R-C1-007 — typed patch contract

A `PatchSet` binds one `WorkspaceId`, expected `Generation`, expected `RevisionNumber`, stable patch
identity, and a canonical nonempty list of unique operations. Create, replace, and delete operations
carry an exact absent/present preimage expectation. Present preimages bind SHA-256, byte length, and
portable executable/regular mode. Final content binds SHA-256, size, bytes, requested mode, and an
explicit preserve/LF/CRLF line-ending policy. Bounds and checked arithmetic are enforced before I/O.

### R-C1-008 — deterministic patch planning

Verified code validates ordering, duplicate targets, workspace identity, generation/revision,
operation legality, preimage/result consistency, content digests, line-ending transformation, and
the exact ordered filesystem actions. The effect shell cannot accept an unplanned `PatchSet`.

### R-C1-009 — transactional patch application

The patch adapter verifies current preimages, creates an operation directory beside protected
workspace metadata, writes and syncs every final file, persists a bounded transaction manifest,
moves originals to transaction backups, installs finals, syncs affected directories, and removes
the transaction only after complete verification. An ordinary failure rolls back completed steps
in reverse order. Apply reports `Applied` only after every requested result is re-read and matched.

### R-C1-010 — patch restart recovery

Transaction manifests use explicit prepared/installing/installed phases and exact path/digest
records. Recovery validates the manifest and actual files, then deterministically reports and
applies one of: already applied, rolled back cleanly, dirty, or indeterminate. Corrupt or mismatched
transaction metadata is quarantined and never interpreted as success.

### R-C1-011 — explicit workspace state

`peritus-workspace` owns a move-only writable workspace and a separate read-only snapshot type.
State binds `WorkspaceId`, `ResourceId`, canonical root, lineage baseline, `Generation`,
`RevisionNumber`, current tree, and clean/dirty/reconciling/indeterminate status. Writable handles
cannot be cloned; read-only handles expose inspection and snapshot identity but no mutation method.

### R-C1-012 — target-owned authorization gateway

`WorkspaceAuthorizationRequest` contains the requested workspace/action identity and borrows exact
C0 `CommittedKernelTransition`, `CommittedCapabilityUse`, and `CommittedLeaseTransition` values.
The workspace gateway verifies the B0 current action is authorized and matches action ID/digest,
actor, role, environment, current `RevisionTuple`, resource, and capability; the committed B1
capability use matches those fields; and the committed B1 lease transition is an active use for
the same action, digest, workspace, resource, environment, holder, generation, and unexpired
authority observation. Only this check constructs a crate-private, move-only `MutationPermit`, and
the permit is consumed immediately by the target operation. No raw patch/Git mutation API accepts
caller-constructed authorization flags.

### R-C1-013 — exact resource identity refinement

Opening a workspace binds the configured `ResourceId` to the canonical workspace root and lineage.
Every resolved target retains that binding. Authorization compares the target binding, B0 witness,
capability permission, lease scope, and workspace state by exact nominal identity before effect.
The executable rule and its proof discharge `REF-C1-B1-RESOURCE-IDENTITY`.

### R-C1-014 — generation-fenced lease enforcement

Every mutation checks the exact current committed lease use and workspace generation immediately
before its first filesystem action. Candidate creation and rollback also require the same gateway.
A renewed, released, expired, fenced, quarantined, retired, different-holder, or different-version
claim cannot be reused. Logical B1 claims and uncommitted CAS observations are insufficient.

### R-C1-015 — restart reconciliation refinement

After a fence or restart, C1 compares an exact `ReconciliationCorrelation` with the expected
workspace lineage, prior holder, fenced generation, transaction manifests, Git HEAD/tree/status,
and last committed snapshot. The C1 restart classifier yields typed `Clean`, `Dirty`, `Fenced`, or
`Indeterminate` outcomes with evidence digests over the complete observation; only an exactly
correlated `Clean` result can contribute resource-safety evidence to B1's later `SafeToAcquire`
decision. It never treats absence of a known process as workspace safety. This rule discharges
`REF-C1-B1-RECONCILE-SAFETY`; C2 remains responsible for holder-quiescence evidence.

### R-C1-016 — artifact and journal integration

Candidate and reconciliation manifests are canonical bytes finalized through
`peritus-artifact-store`. Outcomes expose the artifact digest, tree ID, prior/current revision,
action identity, patch identity, and reconciliation evidence needed for E0 to append the matching
B3/C0 lifecycle result. C1 consumes C0 receipts for authorization but does not forge receipts,
append substitute lifecycle events, or expose a mutation result as accepted evidence.

### R-C1-017 — bounded errors and diagnostics

All three crates expose stable error codes, operation context, recovery class, and bounded source
details. Expected errors include invalid repository, Git unavailable/protocol failure, object
mismatch, worktree conflict, dirty worktree, invalid/protected/symlink path, stale workspace,
preimage mismatch, atomic rollback failure, interrupted transaction, authorization mismatch, stale
lease, and indeterminate reconciliation. Malformed input and normal environmental failures do not
panic.

### R-C1-018 — maintainability and formal coverage

The three crates use responsibility-based modules, composition-only roots, typed public APIs, no
unsafe code, no generic dumping-ground modules, and no reachable placeholders. All deterministic
planners, validators, state transitions, authorization comparisons, reconciliation classifiers,
and digest-input construction supported by Verus are verified and listed in the obligation
inventory. Git/filesystem adapters are class-H boundaries exercised by real integration tests.

## Acceptance criteria

1. `peritus-git`, `peritus-patch`, and `peritus-workspace` exist under `crates/runtime/`, are class
   H, build in ordinary Rust and Cargo Verus, remain within source-layout limits, and have complete
   crate documentation and typed errors.
2. Real-repository tests open SHA-1 repositories, resolve immutable baselines, create detached
   writable/read-only worktrees, parse clean and dirty porcelain-v2 status, create candidate trees,
   retain snapshot refs, restore snapshots, and remove only exact registered worktrees.
3. Patch tests cover create/replace/delete, executable mode, LF/CRLF/preserve behavior, exact
   preimages, multiple-file success, stale generation/revision, duplicate paths, and all configured
   size/count bounds.
4. Failure injection before and during prepare, backup, install, directory sync, and cleanup proves
   ordinary failures leave originals restored or a restart-visible transaction; no partial result
   is returned as applied.
5. Recovery tests exercise prepared, partially installed, fully installed, corrupt-manifest,
   tampered-backup, and missing-file states and produce exact applied/rolled-back/dirty/indeterminate
   outcomes.
6. Path tests cover absolute paths, traversal, alternate separators, reserved names, protected
   `.git`/`.peritus` metadata, symlink files/directories, nested repository/worktree metadata, and
   normal nested file operations.
7. Workspace authorization tests independently drift every action, actor, role, environment,
   revision, resource, capability, lease holder, lease generation, lease version, and authority
   time field and prove no target effect occurs.
8. Compile-fail/API tests prove callers cannot construct or name the private mutation permit and
   cannot invoke patch, candidate, rollback, or writable Git operations without the workspace
   gateway.
9. Integration tests prove an exact authorized patch mutates the intended workspace, advances one
   logical revision, creates a Git tree and finalized manifest artifact, and becomes observable
   after reopening.
10. Read-only snapshot tests prove inspection works and mutation is absent from its API; a reviewer
    snapshot never shares the writer's live writable directory.
11. Rollback tests prove content restoration creates a successor revision, retains both candidate
    trees/manifests, and leaves the baseline and user branch unchanged.
12. Reconciliation tests cover clean, tracked dirty, untracked, ignored, conflicted, interrupted
    patch, mismatched correlation, and unverifiable Git states, producing only the documented typed
    dispositions.
13. Production C1 Git/workspace/patch conformance cases are added to A2 and pass against the real
    implementation using deterministic identities/faults and temporary repositories.
14. Named Verus rules and executable refinement tests discharge
    `REF-C1-B1-RESOURCE-IDENTITY`, `REF-C1-B1-RECONCILE-SAFETY`, and
    `REF-C1-B1-AUTHORITY-GATE`; those reservations are removed only with complete manifest evidence.
15. Cargo, lockfile, architecture policy, verification manifests, ordinary-API policy, CI,
    documentation, and generated inventories register every new crate and formal source.
16. Focused tests, strict rustdoc/Clippy, architecture/source-layout/trust/API checks, full
    Verus/no-cheating verification, `just check`, and `just gate-a` pass.
17. Bounded QA finds no unresolved correctness, authority, recovery, or maintainability defect.
18. The reviewed change is signed, pushed, merged through the protected-main pull-request path,
    exact `origin/main` contains the delivery commit, and Crosslink issue #10 is closed.

## Current architecture

B0 exposes an `ActionAuthorizationWitness` only inside the exact current `ActionState` of a
`KernelAggregate`. C0's move-only `CommittedKernelTransition` exposes that post-commit aggregate.
B1 capability-use transitions bind action, digest, actor, role, environment, `RevisionTuple`,
resource, capability, time, and successor. B1 lease transitions bind workspace/resource/environment,
holder, generation, claim version, action use, and reconciliation correlation. C0 separately
returns move-only committed capability and lease transitions. These are sufficient for C1 to
cross-check authority without inventing an E0 witness.

C0 already provides transactional journal receipts and content-addressed artifact finalization.
B3 provides bounded canonical framing and immutable lifecycle/action representations. A2 provides
real temporary directories, deterministic identities, faults, and extensible conformance catalogs.
There are no runtime-layer crates yet, no production Git/workspace/patch API, and all three C1
refinement reservations remain open.

## Proposed design

### Crate boundaries

```text
peritus-patch     -> peritus-types, peritus-codec, sha2
peritus-git       -> peritus-types, peritus-codec, sha2
peritus-workspace -> peritus-git, peritus-patch, peritus-kernel, peritus-policy,
                     peritus-leases, peritus-journal, peritus-artifact-store,
                     peritus-codec, peritus-types
```

All three crates are runtime-layer class H. `peritus-patch` does not depend on Git, and
`peritus-git` does not depend on patching. `peritus-workspace` is the integration owner and the
only public mutation surface.

### `peritus-git` modules

```text
src/lib.rs
src/error.rs
src/object_id.rs
src/name.rs
src/command.rs
src/repository.rs
src/baseline.rs
src/worktree.rs
src/status.rs
src/status/porcelain.rs
src/snapshot.rs
src/reconcile.rs
src/verified.rs
```

`GitCommand` is an internal argv plan. The runner uses `std::process::Command` directly; C2 later
owns agent-controlled process execution, but C1's fixed Git plumbing is part of this target adapter.
Reads use plumbing/porcelain designed for scripts and NUL delimiters. Mutations always use exact
validated object IDs and paths supplied as separate argv entries after `--` where supported.

Snapshot creation uses the isolated worktree index to add allowed content and `write-tree` to
produce the content identity. A synthetic snapshot commit plus `refs/peritus/workspaces/<id>/...`
retains trees without touching user refs. Commit metadata is fixed by C1 inputs so retries are
idempotent; the tree ID remains the authoritative content identity.

### `peritus-patch` modules

```text
src/lib.rs
src/error.rs
src/path.rs
src/content.rs
src/operation.rs
src/set.rs
src/plan.rs
src/preimage.rs
src/line_endings.rs
src/transaction.rs
src/transaction/manifest.rs
src/transaction/apply.rs
src/transaction/recover.rs
src/verified.rs
```

Patch planning is independent of the filesystem. The transaction adapter receives a canonical
root, a separate protected transaction root, and a checked plan. The transaction root is never
inside the agent-visible workspace and is derived from the patch identity, not arbitrary input.
The manifest is an implementation recovery format with explicit versioning and atomic replacement;
it is not an authority record.

### `peritus-workspace` modules

```text
src/lib.rs
src/error.rs
src/identity.rs
src/state.rs
src/open.rs
src/read_only.rs
src/writable.rs
src/authorization.rs
src/gateway.rs
src/mutation.rs
src/candidate.rs
src/rollback.rs
src/manifest.rs
src/reconcile.rs
src/refinement.rs
src/verified.rs
```

`WorkspaceGateway` owns a writable workspace handle. Public mutation methods accept a
`WorkspaceAuthorizationRequest` and operation input, authorize internally, consume the private
permit, execute the checked effect, reconcile Git, finalize the outcome manifest in C0's artifact
store, and return a move-only outcome. It never returns the permit. Read-only snapshots use a
separate type containing only repository inspection operations.

### Mutation sequence

```text
committed B0/B1/C0 observations
        -> verified target authorization
        -> crate-private one-use permit
        -> checked patch/Git plan
        -> filesystem transaction
        -> Git reconciliation and tree creation
        -> artifact manifest finalization
        -> move-only outcome for later E0/B0/C0 recording
```

If the effect completes but artifact finalization fails, the workspace remains an observable dirty
or recoverable result and the error contains the exact recovery route. C1 does not roll back a real
completed edit merely to make telemetry publication look atomic.

### Parallel delivery boundaries

After this design freezes shared types, three workstreams may proceed without overlapping source
paths: `peritus-git`, `peritus-patch`, and `peritus-workspace`. The workspace workstream initially
compiles against the documented public surfaces and performs integration after the two leaf crates
land. Root Cargo/architecture/verification/A2 files have one integration owner.

## Data and compatibility

Git object IDs support the repository's reported SHA-1 or SHA-256 format and are stored with an
explicit algorithm tag. C1 does not reinterpret or shorten object IDs. Snapshot manifests and
patch transaction manifests start at schema version one, use canonical length-delimited bytes, and
carry digests. Their decoders reject unknown versions and trailing bytes.

No production C1 data exists. C0 schema changes are unnecessary: snapshot manifests are ordinary
finalized artifacts, while lifecycle authority remains in existing B0/B1/C0 events. Future manifest
versions are additive or migrated explicitly; old artifacts remain readable.

## Failure handling

Failures are separated into invalid request, stale authority/state, Git protocol/environment,
filesystem transaction, recovery required, dirty workspace, and indeterminate outcome. An error
never silently converts dirty or unknown state to clean. Bounded stderr and path context remain
available for diagnosis without leaking file contents by default.

Drop implementations release process/file handles and best-effort temporary resources but do not
claim recovery. Explicit close/remove/recover methods return observed results. Interrupted work is
resolved from manifests plus actual Git/filesystem inspection on restart.

## Security considerations

Model-controlled strings never become shell input, Git options, absolute paths, refs outside the
C1 namespace, or paths below protected metadata. A leading-dash path is still passed after `--`.
Repository-selection environment variables are removed from Git subprocesses. Mutating operations
reject symlink targets/components and nested `.git` metadata. Read-only and writable worktrees are
distinct directories and Rust types.

C1 provides portable lexical, canonical, symlink, metadata, authority, and generation checks.
Platform-native sandbox enforcement and handle-relative race resistance are strengthened by C3;
C1 fails closed on a path shape it cannot safely represent instead of silently granting access.

## Verification

Focused development commands are:

```text
cargo test --package peritus-git --package peritus-patch --package peritus-workspace --all-targets --all-features --locked
cargo clippy --package peritus-git --package peritus-patch --package peritus-workspace --all-targets --all-features --locked -- -D warnings
cargo doc --package peritus-git --package peritus-patch --package peritus-workspace --all-features --locked --no-deps
cargo verus verify --package peritus-git --package peritus-patch --package peritus-workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo xtask all
just gate-a
```

Tests use real temporary Git repositories/worktrees and filesystem trees. A deterministic local
fault port injects failures only at named transaction boundaries; it does not replace Git or file
I/O. Compile-fail documentation tests cover private authority surfaces.

## Rollout and rollback

C1 lands before a daemon or user-facing CLI consumes it. Rollout adds the three libraries,
conformance cases, manifests, and documentation in one protected-main change. There is no database
migration. Before a consumer lands, rollback is removal of C1 registrations. After consumers land,
old snapshot and patch manifests remain readable and incompatible behavior requires an explicit
format revision.

## Open questions

None block implementation. Concrete C2 process ownership and C3 native sandbox backends integrate
through later adapters without changing C1's checked plan and gateway contracts.

## Out of scope

- General shell/process/PTY execution, cancellation, resource controls, and sandbox policy (C2).
- Native Linux/macOS/Windows sandbox, network, and secret backends (C3).
- Built-in model-facing filesystem, Git, shell, and quality tools (C4).
- Gate execution, writer/reviewer/fixer orchestration, daemon startup fencing, and public CLI/TUI
  commands (D1/E0/G0-G2).
- Merging an accepted candidate into a user branch; C1 produces and preserves candidate objects,
  while a later explicitly authorized delivery operation owns user-branch integration.

## Alternatives considered

Using `libgit2` through the `git2` crate would avoid subprocess parsing, but adds a large FFI
boundary, platform build surface, independent Git-behavior differences, and a public dependency
that is harder to verify. Structured calls to the installed Git executable match the repositories
users operate and keep command construction auditable, so they are preferred.

Applying files directly with no transaction manifest would be shorter, but cannot give meaningful
multi-file rollback or restart reconciliation. C1 therefore uses a small versioned transaction
directory with staged finals and backups. A full virtual filesystem or copy-on-write mount would
provide stronger global atomicity but belongs to platform sandbox work and is unnecessary for the
normal patch loop.
