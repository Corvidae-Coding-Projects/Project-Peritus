# peritus-workspace

Target-owned authorization, mutation orchestration, immutable snapshots, rollback, and restart
reconciliation for isolated Peritus workspaces.

The crate is the sole public C1 mutation boundary. `WorkspaceGateway` owns isolated Git-worktree
mutation. `FolderMutationGateway` separately owns exact-preimage patches against one registered
ordinary folder without fabricating Git state. Both cross-match exact committed B0 and B1 receipts
before consuming a private operation permit. Read-only snapshots use a separate type and never
share a writer's live worktree.

Permit consumption is durable per workspace generation and revision. Before the first target
effect, the writable workspace exclusively creates and synchronizes a bounded action marker under
its separate transaction root. The marker binds workspace, resource, environment, counters,
action ID, and action digest. `WritableWorkspace::open` reloads and validates those markers, so
reconstructing a `WorkspaceGateway` does not make a consumed action reusable. Malformed or escaped
ledger state fails closed.

The transaction root is a dedicated canonical namespace with a synchronized binding manifest for
the exact workspace, resource, and environment. It may not overlap the worktree or Git common
directory in either direction. Restart recovery considers only exact `txn-` plus 64-lowercase-hex
directories; unrelated entries make the observation dirty without being renamed or quarantined.

An authorized patch leaves the workspace dirty until a separately authorized candidate operation
creates a Git tree and retained snapshot, finalizes the canonical workspace manifest through the
artifact store, and installs the successor revision. Rollback likewise restores a retained
same-lineage snapshot as a new successor; once restoration changes the worktree, later failures
leave the workspace dirty or indeterminate for reconciliation rather than reporting it clean.

Restart reconciliation supplies the current workspace tuple to patch recovery through
`RecoveryBinding`, inspects Git against the retained current snapshot, and produces one of clean,
dirty, fenced, or indeterminate. The durable action ledger is target metadata, not a patch
transaction, and is excluded from transaction recovery scans.

## Streaming inspection

File inspection hashes the complete source as a stream and resolves exact byte or line
selections without a source-size ceiling. `read_file` collects bytes using the capacity
explicitly supplied by its caller; C1 imposes no additional inclusion ceiling.
`copy_selection` writes any selection to caller-owned storage with fixed working memory.
`capture_file` seals a reader over a distinct caller-owned regular file. Its pages carry
the actual original-source interval, complete observation metadata, and a continuation
cursor whenever content remains. Physical 64 KiB pages do not limit total content or
page count. Line selections also accept full-width counters through `lines_u64`.

The owner persists retained content, `InspectedSelection::encode()` metadata, and
`InspectionCursor::encode()` continuation under its own durable publication transaction.
After restart, decode both records, verify content with `RetainedInspection::open`, and
continue from the same offset. Reopening checks the complete selected digest once;
page reads check the opened storage version and never substitute current source bytes.
Inspection metadata conveys observation only, not consent or mutation authority.
Failed capture produces no accepted observation; partial storage remains owned by the
caller. Existing attachment, transport, and filesystem-tool policies remain separate
from this C1 streaming contract.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-workspace
```
