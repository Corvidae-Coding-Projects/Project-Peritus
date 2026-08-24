# peritus-tools-git

Production structured Git tools for Peritus's model-facing C4 boundary.

The canonical catalog contains `git.status`, `git.diff`, `git.history`, `git.candidate`,
`git.snapshot`, `git.rollback`, and `git.merge`. Status, diff, history, and current/retained snapshot
inspection use C1 typed observations tied to an immutable `ReadOnlyWorkspace`; no model-provided
shell or unrestricted Git argument string is accepted. File, commit, tree, snapshot, reference,
manifest, and repository identities remain structured and rendering has independent bounds.

`git.candidate` invokes the C1 operation that atomically creates a candidate and retained successor
snapshot. `git.snapshot` is observation-only. `git.rollback` restores a retained snapshot as a new
successor through `WorkspaceGateway`. These effects exist only in `GitDispatcher::start`, which
requires a router-created `AuthorizedInvocation` and exact-matches its validated C4 caller binding
against the independently committed C1 authorization before effect.

`git.merge` is registered with its repository-history mutation class but returns a stable typed
unsupported result without receiving a ref-mutation handle. It remains unavailable until C1 owns a
separately authorized user-branch delivery operation.
