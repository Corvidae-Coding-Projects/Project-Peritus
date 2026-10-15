# peritus-tools-git

## Folder guide

Authorized structured Git tools for Peritus workspaces.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-git`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Structured Git tools for immutable and authorized Peritus workspaces. |

Production structured Git tools for Peritus's model-facing C4 boundary.

The canonical catalog contains `git.status`, `git.diff`, `git.history`, `git.candidate`,
`git.snapshot`, and `git.rollback`. Status, diff, history, and current/retained snapshot
inspection use C1 typed observations tied to an immutable `ReadOnlyWorkspace`; no model-provided
shell or unrestricted Git argument string is accepted. File, commit, tree, snapshot, reference,
manifest, and repository identities remain structured and rendering has independent bounds.

`git.candidate` invokes the C1 operation that atomically creates a candidate and retained successor
snapshot. `git.snapshot` is observation-only. `git.rollback` restores a retained snapshot as a new
successor through `WorkspaceGateway`. `GitDispatcher::start` requires a router-created
`AuthorizedInvocation` and transfers the workspace, artifact store, and original committed
receipts to an owned worker. The worker matches the validated C4 caller binding against the C1
authorization before any effect. Polling observes completion; cancellation or router teardown
cancels and joins the worker and its Git process tree.

Diff pages return the immutable base, target, and complete observation digest. Continue with
`expected_digest`, `entry_offset`, `patch_offset`, and `path_byte_offset`; native path bytes and
patch bytes are base64 encoded, while the path label is only a safe display preview. A null next
offset marks completion of that stream. Pages fit the selected encoded output budget. Candidate
and rollback results are recoverable from durable operation receipts even when later result
publication fails, without repeating an already completed mutation.

Branch delivery remains unavailable until C1 owns a separately authorized user-branch delivery
operation, so no merge capability is advertised.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
