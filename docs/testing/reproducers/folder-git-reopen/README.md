# Folder-to-Git reopen regression

A browser-building conversation started in a trusted plain folder. The agent later
initialized Git there. On reopening, the launcher discovered a Git repository,
replaced the remembered folder profile with a new restricted Git profile, and
changed its workspace ID. The saved run and conversation retained their original
workspace ID. The actual TUI then rejected reattachment with `session-mismatch`:
"The request belongs to a different workspace or session. Reopen the workspace and
retry."

The launcher now observes the canonical folder identity before Git discovery. An
exact remembered direct-folder match retains its workspace, trust, and in-place
mode even when Git is added, including discovery from a repository subdirectory.
A path with a different filesystem identity does not
inherit that registration. New Git repositories still follow normal Git discovery.
Explicit workspace-management operations are unchanged.

## Replay through the real TUI

Build the CLI, and put it beside the matching-version daemon. Then run:

```sh
python3 docs/testing/reproducers/folder-git-reopen/replay.py \
  --binary /path/to/candidate/bin/peritus --output /tmp/folder-reopen-results.json
```

To independently reproduce the original regression with the published 0.0.7 package:

```sh
python3 docs/testing/reproducers/folder-git-reopen/replay.py \
  --binary /path/to/published-0.0.7/bin/peritus --expect-mismatch
```

The driver operates workspace trust and offline provider selection through actual
terminal prompts. Each disposable sequence opens a plain folder, retains its
profile, runs real `git init`, and reconnects a fresh CLI. The independent expected
result is unchanged workspace ID, folder identity, trust, and original file bytes.
Cases cover a normal path, Unicode/spaces, implicit current-directory discovery,
and reopening from a nested directory after Git initialization.
The legacy replay instead requires the specific changed-ID/restricted-Git result.
Only fixture daemons are shut down; no coding runs or AI requests are started.

## Recovery through the public API

`recover.rs` is a minimal consumer of `ProductBootstrap::configure_workspace`.
It imports only the specified direct-folder profile from a previous immutable
product-state generation. It first checks the folder's current public
`FolderIdentity` digest against that profile and rejects a replacement directory.
It then publishes a new state/configuration generation through Peritus's normal
locking and persistence API. It neither rewrites the run nor changes providers.

Build it as a separate consumer with path dependencies on `peritus-launcher`,
`peritus-product-state`, and `peritus-workspace`, plus the workspace's pinned
`serde_json`. Invoke it with a trusted prior state file and its exact workspace ID:

```sh
peritus-folder-recovery /path/to/state-before-reclassification.json WORKSPACE_ID
```

## Observed October 12, 2026

The reported failure was reproduced through the installed TUI. Restoring the
original folder through the public consumer left the browser run record byte-for-byte
unchanged. A fresh updated CLI attached the exact saved run, displayed its recorded
activity, and reported recovery required. Reopening the exact run through both its
normal and canonicalized paths preserved the restored workspace identity and showed
the history. The final binary also reattached this saved run from the project root
and `crates/browser-engine`. Four disposable TUI/Git/reopen sequences passed,
including the Unicode path, implicit discovery, and nested-directory cases.
The published 0.0.7 binary reproduced the specific changed-workspace-ID/restricted-Git
regression in the three original sequences.
The nested case additionally exposed a missing repository-root check in the initial
fix; that concrete replay now passes after validating the discovered root against
the remembered folder identity.

The final installed CLI SHA-256 was
`a8e88eec12131f29264037f9287a71938e2e8e5d3f5ab824f1589010698d428a`.
It includes this fix and the release-metadata fix; the daemon remains published
0.0.7. Evidence and private backups are under
`/home/doll/Peritus-session-recovery-2026-10-12`.

The saved browser run remains recoverable and was not restarted by these checks.
The real reattachment checks cover opening the exact saved run; they do not claim
every conversation-library UI behavior or migration of all historical state.
No unit tests, mocks, or repository/formal verification suites were run.
