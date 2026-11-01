# Stage 6 filesystem and scoped read contracts

The `filesystem` target runs actual `peritus-patch` operations in a disposable workspace with a
separate same-device transaction root. A four-slot byte/mode table is the independent reference
model. Each history step builds a nonempty one-to-four-file patch from the current model, then
tries ordinary application, stale workspace version, changed preimage, cancellation before first
mutation, or a completion persistence failure followed by wrong-binding and exact-binding
recovery. The model compares target bytes and portable executable mode after every step. It also
checks that an unrelated workspace file and an outside sibling file retain their exact bytes.
The patch target asserts canonical path order, exact completion/patch identity, rejection before
effect for stale/drifted plans, terminal cleanup, and source-bound restart recovery. A simulated
completion failure is an I/O callback result, not a process crash or power loss. The public patch
API only observes cancellation before its first workspace mutation; after that, cancellation is
deferred. Its mutation fault hooks are private, so this campaign does not claim a deterministic
post-mutation cancellation observation.

The `scoped_read` target opens the actual `FolderInspection` capability behind filesystem reads
on a disposable directory. Independent byte and one-based inclusive line cursors determine
selected bytes, half-open ranges, continuation offsets and errors. Direct SHA-256 over the source
and included bytes checks both digests. Histories include same-handle rewrite, full scans,
pre-scan cancellation, UTF-8 byte splits, binary bytes, exact EOF and Unix no-follow symlinks.
The outside symlink target is a canary. This target does not substitute for C4 `FsDispatcher`
authority or concurrent-write race testing.

The `fs_snapshot` target creates a disposable Git repository, commits exact bytes, creates a
read-only linked worktree through `GitRepository`, opens it through `ReadOnlyWorkspace`, and reads
with the public `FsReadService`. It compares every file page, UTF-8 or independently encoded
base64 content, both SHA-256 identities, metadata and continuation to the committed byte vector.
It mutates the writer after the first read and checks that the snapshot remains byte-identical and
Git-clean. A request that names the same snapshot root as the writer is rejected. This covers the
FS tool service's real Git-backed read path.

The `fs_dispatch` target uses a separate disposable Git-backed snapshot and the canonical
`fs.read` descriptor through `ToolRouter::prepare`, committed B0/B1/budget/epoch authority,
`ToolRouter::dispatch`, and the actual `FsDispatcher` read worker. An independent byte model
checks the structured page, content encoding, digests, range, continuation and metadata. A
writer change after snapshot creation must remain invisible. An authority request with a wrong
prepared digest must be rejected before dispatch, and exact replay must return the same terminal
result. This covers one admitted read page per case, not all FS operation kinds or concurrent
writer races.

Source disposition for this lane:

| Owner | Stage 6 disposition |
| --- | --- |
| `peritus-patch` plan/apply/recover | Direct generated native file-tree and transaction histories |
| `peritus-workspace` `FolderInspection` | Direct opened-folder selection, scan, cancellation and no-follow histories |
| `peritus-workspace` mutation gateway | Caller/recovery path inspected; authenticated one-use gateway is not yet directly exercised by this lane |
| `peritus-tools-fs` | Public read service and direct C4 `fs.read` router/dispatcher admission exercised through separate real Git-backed snapshots; other FS operation kinds remain |
| `peritus-git` | Open/baseline/create/inspect worktree path exercised; broader raw Git diff/snapshot/candidate correspondence remains |
| `peritus-tools-shell` and `peritus-tools-quality` | Execution-plan compilers inspected; exact argv and discovery/execution still require a separate admitted process campaign |

Calibration patches change one production condition each and remain unapplied in the source:
`stage6-filesystem.patch` ignores an exact revision mismatch in `PatchSet::plan`;
`stage6-scoped_read.patch` rejects an otherwise valid byte selection ending exactly at EOF;
`stage6-fs_dispatch.patch` drops the final byte of a requested FS read page.
The retained fixed examples and targeted generated cases must fail under those mutations and
pass after exact source restoration.
