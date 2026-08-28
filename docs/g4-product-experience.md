# G4 product experience

G4 composes Peritus into one local product command. The ordinary interface owns platform paths,
workspace selection, trust, provider setup, daemon lifecycle, coding runs, and the terminal UI:

```text
peritus
```

No normal flow requires an IPC endpoint, an environment export, an internal identifier, or a
hand-written TOML file. From a source checkout, `cargo xtask product-install` creates and installs
the checked native package first.

## First launch

Peritus discovers the current Git repository, explains the exact repository it found, offers a
restricted default, and remembers completed choices. A trusted repository receives a separate
application-managed detached worktree below Peritus state; the user's current checkout is never the
agent's writable target.

Provider onboarding shows every available route and readiness state. Account-backed Codex and
Claude routes delegate login and requests to the official `codex` and `claude` executables, which
remain the credential owners. Direct OpenAI, Anthropic, Gemini, and compatible routes use hidden
credential input and the operating-system credential store. Peritus stores only provider settings
and opaque credential references. Offline mode remains available for inspection.

Completed setup is resumable. Repeat launch skips healthy decisions, repairs only the provider or
workspace that needs attention, regenerates immutable daemon configuration when settings change,
and starts or reuses the packaged local daemon before entering the UI.

The launcher records both the immutable daemon configuration and the installed daemon executable
digest. After an upgrade it waits for an older daemon to withdraw its endpoint and release its
instance lock before starting the replacement. If a connection later drops, the offline status
displays `R restart/reconnect`; that action returns daemon ownership to the launcher, restores
readiness, and resumes the durable application session.

## Coding runs

The Runs view is the ordinary work surface:

1. Press `n`, describe the desired coding outcome, and press Enter. Shift-Enter adds a line.
2. Peritus sends the task and the selected writer, reviewer, and fixer providers to the daemon.
3. The production D0 writer inspects and searches the real managed worktree, makes bounded edits,
   runs structured commands, observes build/test failures, and retries before reporting readiness.
4. D1 maps every exact changed file to its nearest Rust, Node, Python, or Go project manifest and
   runs that project's explicit compile/test/lint commands. Uncovered files, missing commands,
   failed commands, or an empty candidate all refuse acceptance; an unrelated root project cannot
   satisfy a nested game or package.
5. The independent D2 reviewer returns typed findings. Policy derives blocker status, so
   correctness, requested-behavior, build-coverage, test-coverage, and security findings block
   regardless of the reviewer's severity wording. Findings remain open across daemon persistence
   until a fixer addresses them and a fresh reviewer confirms their absence.
6. E0 sends all failed checks and conserved findings to the tool-capable fixer and repeats fresh
   gates and review within an explicit bounded cycle budget.
7. The run completes only when exact-target gates pass and no conserved policy blocker remains.
   Completion retains the original task-level result plus any verified fixer summaries.

Every run is also a durable conversation. Select it and press Enter or `m` to reply, redirect, add
context, or say continue; Shift-Enter adds a line. Messages sent during active work are incorporated
at the next safe writer or reviewer boundary. Messages sent to a Failed, Cancelled, Recovery
required, Waiting for user, or Complete run resume the same managed worktree with the entire
conversation. When the writer genuinely needs a material choice it asks one direct question and
enters Waiting for user instead of guessing or abandoning the run.

Peritus admits one active coding run per managed worktree, preventing two model loops from editing
the same files at once. A completed but uncommitted deliverable also reserves that worktree until
it is committed or discarded, preventing a later task from silently absorbing its files. Other
configured workspaces remain independently usable.

The daemon persists each visible phase: Queued, Writing, Checking, Reviewing, Fixing, Verifying,
Waiting for user, Complete, Failed, Cancelled, or Recovery required, together with its bounded
conversation. The Runs view shows that state as text as well as color, the Diff view shows tracked
and newly created text files, and the Review view shows the latest review or repository checks.
After completion, the handoff shows the managed path, exact changed-file count, exact successful
commands, and how to run the result. Press `i` to inspect, `a` to accept, `c` to commit only the
deliverable paths, `p` to export an exact patch, or uppercase `D` to discard only those paths.
Press `x` to cancel a selected run and `r` to retry a failed, cancelled, or interrupted run. A
daemon restart marks an unfinished run Recovery required rather than pretending it completed;
durable typed findings remain conserved on retry.

Provider roles default to the selected provider. In the Runs view, press `w`, `e`, or `f` to cycle
the writer, reviewer, or fixer independently before starting the next task. Press `?` for the full
keyboard reference. Existing G2 trace, evolution, approval, and terminal views remain available.

## Workspace status and settings

The workspace menu uses plain path labels and text status:

- `Ready` means the managed copy is present and clean.
- `Ready — changes in progress` means the managed copy has uncommitted work and is preserved.
- `Restricted` means the source repository is remembered without execution authority.
- `Needs repair` means a retained path, repository identity, worktree, or registration no longer
  matches.

Forgetting a trusted entry removes it from the recent list but retains the registered managed copy
for safe daemon recovery and later product cleanup. It never silently discards unfinished changes.
An interrupted registration publication is recovered from the exact managed worktree rather than
creating another copy.

Focused settings commands are:

```text
peritus open [PATH]     Open an explicit repository, defaulting to the current directory
peritus providers       Add, remove, repair, or select providers
peritus workspaces      Switch, add, trust, repair, or forget workspaces
```

Interactive product commands require terminal input and output; they return a usage error instead
of waiting indefinitely in a pipe or CI job. The existing explicit daemon and protocol commands
remain available for automation.

## Native packaging and lifecycle

`cargo xtask product-package` builds the host's release binaries and writes a native package below
`dist/peritus-<platform>-<architecture>`. The package contains `peritus`, `peritusd`,
`peritus-tui`, the platform sandbox helper, lifecycle scripts, a canonical manifest, and exact
SHA-256 checksums. Generated binaries stay under ignored build/output directories and are never
checked into Git.

`cargo xtask product-install` builds that package and installs it for the current user. Install and
upgrade verify every staged artifact before publishing package-owned files. Upgrade snapshots and
restores only package-owned files if installation fails. Ordinary uninstall removes the command,
daemon, TUI, helper, optional supervisor template, and legacy supervisor registration while
preserving provider credentials, configuration generations, managed worktrees, run state, logs,
and diagnostics.

Hosted Linux, macOS, and Windows gates assemble native packages from already checked build outputs,
exercise install, repeat command launch, upgrade, and uninstall, and assert protected-state
preservation. The production package command separately builds optimized locked artifacts.

## Local state

Peritus selects native per-user locations automatically:

- Linux: XDG config/state/cache roots, falling back to `.config`, `.local/state`, and `.cache`.
- macOS: Application Support plus the native user cache directory.
- Windows: roaming configuration plus local state/cache directories.

These roots contain immutable product-state generations, generated daemon configurations, the
public approval registry, managed worktrees, workspace registrations, transaction namespaces,
persisted product runs, and bounded daemon diagnostics. Secret values remain in the operating-system
credential store.

## Ergonomic contract

The accepted interaction rules and their research sources live in
[the G4 design](../.design/single-command-product-experience.md#ergonomic-design-basis). G4 shows
useful defaults, names exact trust targets, uses recognizable paths instead of IDs, avoids unrelated
first-run questions, preserves reversible choices, offers focused repair, keeps status textual
rather than color-only, exposes cancellation and retry beside progress, and never makes the user
reconstruct internal configuration.
