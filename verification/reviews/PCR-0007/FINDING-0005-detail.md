# FINDING-0005 — linked-worktree trust selected the primary checkout

Original severity: high, blocking.

Disposition: fixed.

The installed launcher resolved `HEAD` through the repository's common Git directory. When a user selected a linked worktree whose commit differed from the primary checkout, Peritus created its managed workspace from the primary checkout and could inspect or modify the wrong source.

The reviewed candidate resolves the baseline with the selected worktree's root and per-worktree Git directory while retaining common-directory behavior for bare repositories. The regression gives the linked worktree and primary checkout different commits and file contents. A final installed DeepSeek probe opened a fresh linked worktree, used `workspace_read` and `workspace_search`, and returned the selected source's workspace version `0.0.3`; the pre-fix probe returned the primary checkout's `0.0.1`.
