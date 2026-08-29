# peritus-git

`peritus-git` is Project Peritus's structured Git adapter. It discovers repositories, resolves
immutable commit/tree baselines, creates and inspects detached worktrees, parses porcelain-v2
status, creates candidate trees and retained snapshot commits, and restores snapshot trees.

The crate is deliberately not an authority boundary. It exposes fixed operations rather than a
raw argv runner, never updates user branches, and does not claim that a Git observation authorizes
a workspace mutation. `peritus-workspace` owns authorization and is the only model-facing
mutation surface.

Git is invoked directly without a shell. Repository-selection environment variables are cleared,
configuration that could prompt, sign, page, or run hooks is disabled, and command output is
bounded. Object IDs retain their repository-reported SHA-1 or SHA-256 algorithm.

Repositories that configure external Git clean, smudge, or process filters are intentionally
unsupported. Those programs are outside the bounded subprocess contract, so the adapter rejects
any configured `filter.*.clean`, `filter.*.smudge`, or `filter.*.process` key before status,
checkout, staging, or restoration can execute it. Built-in attributes that do not configure one
of those external filter drivers remain supported.

Registered worktrees and retained candidate snapshots expose bounded schema-v1 manifest bytes.
After process restart, decode those bytes and pass the manifest to `reopen_worktree` or
`reopen_snapshot`. Worktree reopening revalidates repository identity, baseline, filesystem paths,
and detached HEAD. Snapshot reopening revalidates repository identity, commit parent and tree, the
canonical reference name, and the retained ref before recreating a typed handle.

If linked-worktree creation completed but the caller crashed before persisting its registration
manifest, `recover_existing_worktree` accepts the original checked `CreateWorktree` request and
reconstructs a handle only after the existing destination, repository ownership, detached HEAD,
baseline, name, and external-filter policy all revalidate exactly. It is distinct from
manifest-based reopening and never adopts an arbitrary directory.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-git
```
