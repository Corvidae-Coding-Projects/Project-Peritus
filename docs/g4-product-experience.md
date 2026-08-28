# G4 product experience

G4 composes Peritus into one local product command. The ordinary interface owns platform paths,
workspace selection, trust, provider setup, daemon lifecycle, and the terminal UI:

```text
peritus
```

No normal flow requires an IPC endpoint, an environment export, an internal identifier, or a
hand-written TOML file. G4 remains in progress until the interactive task composer and native
package qualification are complete.

## Workspace onboarding

When `peritus` starts, it first looks for a Git repository at the current directory or any parent.
If found, Peritus shows the canonical repository root. If not, it offers remembered repositories
and path entry without treating the current directory as an application failure.

A new repository is remembered in restricted mode. The trust prompt explains the consequence and
names the exact source root. Pressing Enter accepts the displayed default; `n` keeps restricted
mode. Restricted mode does not expose command, build, test, mutation, or agent-execution tools.

Trusting a repository creates a detached writable Git worktree below Peritus's protected local
state directory. The user's current checkout is not the writable agent target. Peritus publishes a
canonical C1 registration binding the repository identity, baseline, workspace identity, managed
root, and isolated transaction root. Repeat launch revalidates those facts and skips the wizard
when they remain healthy.

The workspace menu uses plain path labels and text status:

- `Ready` means the managed copy is present and clean.
- `Ready — changes in progress` means the managed copy has uncommitted work and is preserved.
- `Restricted` means the source repository is remembered without execution authority.
- `Needs repair` means a retained path, repository identity, worktree, or registration no longer
  matches.

Forgetting a trusted entry removes it from the recent list but retains the registered managed copy
for safe daemon recovery and later product cleanup. It never silently discards unfinished changes.

## Commands

```text
peritus                 Open the current repository or last healthy workspace
peritus open [PATH]     Open an explicit repository, defaulting to the current directory
peritus providers       Add, remove, repair, or select providers
peritus workspaces      Switch, add, trust, repair, or forget workspaces
```

The existing explicit daemon commands remain available for automation. Interactive product
commands require terminal input and output; they do not wait for input in a pipe or CI job.

## Provider onboarding

The provider screen labels each route and its current state. Already-authenticated official Codex
and Claude CLIs are selected as useful defaults. Account login is handed to those official
executables, which remain the credential owners. Direct OpenAI, Anthropic, Gemini, and compatible
provider keys use hidden input and the operating-system credential store. Durable product state
contains only provider settings and opaque secret references.

Offline mode remains a deliberate choice. It permits local product inspection but cannot begin a
model-backed coding run.

## Local state

Peritus selects native per-user locations automatically:

- Linux: XDG config/state/cache roots, falling back to `.config`, `.local/state`, and `.cache`.
- macOS: Application Support plus the native user cache directory.
- Windows: roaming configuration plus local state/cache directories.

These roots contain immutable product-state generations, generated daemon configurations, the
public approval registry, managed worktrees, workspace registrations, transaction namespaces, and
bounded daemon diagnostics. Secret values remain in the operating-system credential store.

If provider or workspace settings change while the daemon is running, the next product launch
compares the applied immutable configuration, performs an orderly restart only when necessary,
and then enters the UI. An interrupted registration publication is recovered from the already
created exact managed worktree rather than creating another copy.

## Ergonomic contract

The accepted interaction rules and their research sources live in
[the G4 design](../.design/single-command-product-experience.md#ergonomic-design-basis). In product
terms, G4 applies them by showing defaults, naming operations and exact trust targets, using
recognizable paths instead of IDs, avoiding unrelated first-run questions, preserving reversible
choices, offering focused repair, keeping status textual rather than color-only, and never making
the user reconstruct internal configuration.
