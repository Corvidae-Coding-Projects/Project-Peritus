# Plain-folder workspaces

## Requested outcome

`peritus` and `peritus open PATH` must accept an existing ordinary directory, including a home
directory. Git is not a prerequisite for conversation, inspection, or explicitly requested file
edits. The user chose edits in the selected folder itself, not a managed copy. Never initialize
Git, create a baseline commit, or recursively copy the selected directory as a startup side effect.

## Architecture and compatibility

Keep existing managed Git workspaces and their C1/C0 registration and candidate guarantees intact.
Add an explicitly typed direct-folder profile and daemon declaration. A directory is not a fake
Git registration. Remember trust separately: restricted folders allow conversation and read-only
inspection; trusted folders permit requested in-place effects. Revalidate the selected directory.

The conversational runner needs a direct-folder path independent of Git HEAD, repository-wide
candidate capture, and startup directory-size/ownership scans. Its completed responses do not
claim a qualified Git candidate or support automatic Git accept/commit/discard. Existing checked
Git delivery must remain strict, not silently degrade into an unqualified conversational result.
Trace and effect receipts remain daemon-owned. Native commands retain their explicit raw-effect
semantics; trust is not a claim of filesystem sandbox containment.

A home folder may contain Peritus state. Explicit file tools must exclude protected Peritus
directories, and inspection must not recursively scan all of the home directory before a greeting.
No changes to the user's actual home contents are part of implementation or testing.

Local working memory retains durable observations, bounded retrieval, compaction and checkpoints.
Its direct-folder scope does not capture a Git candidate. File-valid entries use only explicit,
policy-checked dependencies; conversation/task validity remains available. Candidate validity is
rejected instead of claiming a whole-folder snapshot. Private storage may live beneath the selected
folder only inside configured protected roots. Image attachment lookup uses explicit paths only.

Alternative rejected: automatic `git init` or a hidden Git copy. Both impose repository semantics
and broad filesystem work that the user did not request. Another rejected alternative is merely
changing the path prompt, because the runner still requires HEAD after startup.

## Verification

Use temporary non-Git directories for startup/configuration/trust/reopen tests and real daemon
conversation fixtures. Prove a greeting works, requested writes land in the original directory,
restricted and plan/review modes reject mutations, no `.git` appears, and unrelated contents are
unchanged. Preserve legacy Git tests. Run affected tests, strict Clippy, formatting, generated
checks, repository policy checks, and rebuild the actual `peritus` and `peritusd` binaries.
