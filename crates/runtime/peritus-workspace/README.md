# peritus-workspace

Target-owned authorization, mutation orchestration, immutable snapshots, rollback, and restart
reconciliation for isolated Peritus workspaces.

The crate is the sole public C1 mutation boundary. It cross-matches exact committed B0 and B1
receipts before consuming a private operation permit. Read-only snapshots use a separate type and
never share a writer's live worktree.

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

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-workspace
```
