# Command and capability routing

The command directory is mouse-accessible; the same actions can be invoked from
the composer using slash commands. Native actions share the browser dispatcher
and Rust gateway operation journal. CLI-backed actions launch the **installed
Peritus executable in a retained PTY**, not a mock terminal or a shell string.

## Native workspace and conversation controls

| Commands | Native behavior |
| --- | --- |
| `/new`, `/nest` | Create a sibling/child browser session with a distinct daemon run identity |
| `/open [PATH]` | Open a canonical project, or show the project form |
| `/close-project` | Close the selected project tab, retaining its sessions and daemon work |
| `/sessions` | Find and reopen retained browser session tabs |
| `/files` | Show the complete project explorer, paged at 250 entries |
| `/git` | Show real status, branch/remotes, staging, diff, commit, pull, and push controls |
| `/git add [PATH…]`, `/git unstage [PATH…]` | Stage/unstage literal repository-relative paths; omission selects the whole repository |
| `/git diff [PATH…]`, `/git diff-staged [PATH…]` | Inspect unstaged/staged changes |
| `/git commit [-m] MESSAGE`, `/git pull`, `/git push` | Ordinary repository commit, fast-forward pull, push |
| `/git switch NAME`, `/git branch create NAME [START]`, `/git branch rename OLD NEW`, `/git branch delete NAME` | Native branch workflows; deletion confirms and refuses unmerged work |
| `/git remote add NAME URL`, `/git remote set-url NAME URL`, `/git remote rename OLD NEW`, `/git remote remove NAME` | Native remote management; removal confirms |
| `/git fetch [REMOTE]`, `/git pull [REMOTE] [BRANCH]`, `/git push [REMOTE] [BRANCH] [--set-upstream]` | Fetch/prune, fast-forward pull, and publish; no force push |
| `/chat [MESSAGE]`, `/plan [MESSAGE]`, `/review [MESSAGE]`, `/build [MESSAGE]` | Select the explicit interaction mode; an optional message is sent in that mode |
| `/model`, `/effort` | Save per-session model/effort choices; existing conversations update the daemon immediately for subsequent model turns. Providers are fixed after the first message |
| `/details` | Toggle public tool/status details in the conversation |
| `/status`, `/diff` | Inspect the active session's observed run status/candidate diff |
| `/runs` | Inspect recent daemon runs and reopen interactive conversations, including those started by the CLI |
| `/stop`, `/retry`, `/export` | Submit the exact active run's cancel, retry, or export request |
| `/discard` | Show an explicit candidate-discard confirmation for the active run |
| `/settings` | Edit appearance, behavior, layout, viewers, shortcuts, aliases, and TOML |
| `/reconnect` | Reload settings/workspace facts and poll actual daemon/run observations |
| `/help` | Open the searchable command directory |

Session renaming/reparenting/closing, repository selection, file-viewer controls,
and the `.gitignore` preview/apply action also have native mouse and keyboard
controls. Closed sessions keep their work; nesting does not itself fork or copy
model context.

Explorer **Attach to next message** and file drag/drop queue exact text snapshots
for the native web conversation. This is distinct from the `/attach` Workbench
handoff below. Native attachment limits are documented in the WebUI guide.
File preview **View/Edit** and Ctrl/Cmd+S provide local text editing independently
of daemon availability; neither viewing nor editing implicitly attaches a file.

## Exact-run candidate handoffs

| Command | Installed CLI invocation |
| --- | --- |
| `/accept` | `peritus --endpoint ENDPOINT runs accept --run ACTIVE_RUN_ID` |
| `/commit` | `peritus --endpoint ENDPOINT runs commit --run ACTIVE_RUN_ID` |

These commands preserve the native session's exact run ID and delegate
qualification/confirmation to the CLI. They are distinct from `/git commit`.
Managed candidate operations still depend on the daemon's eligibility checks.

## Interactive CLI setup and workbench

| Commands | Routing |
| --- | --- |
| `/terminal` | Open/reopen the selected session console with `peritus --endpoint ENDPOINT open PROJECT_ROOT --run ACTIVE_RUN_ID`; an unconfigured daemon opens project setup |
| `/consoles` | Rediscover retained project consoles after reload, including exited output |
| `/providers` | `peritus providers` for account/API setup and capability checks |
| `/workspaces` | `peritus workspaces` for workspace registration, trust, and repair |
| `/update` | `peritus update` |
| `/approvals`, `/doctor`, `/trace` | Open the workbench CLI with the corresponding slash command prepared |
| `/queue`, `/context`, `/compact` | Same handoff for queued input and context management |
| `/brief`, `/goal`, `/pause`, `/resume`, `/usage`, `/budget` | Same handoff for briefs, bounded goals, and accounting |
| `/preview`, `/checkpoint`, `/rewind`, `/run` | Same handoff for previews, checkpoints, restoration, and execution |
| `/attach`, `/permissions`, `/init`, `/memory`, `/fork` | Same handoff for explicit context attachment, workspace capabilities, guidance, and forks |

Workbench handoffs use `peritus --endpoint ENDPOINT open PROJECT_ROOT --run
ACTIVE_RUN_ID`. The CLI opens that exact conversation, restores its observed
mode/model/provider selection, and presents the requested slash command for
explicit submission. Opening a console does not send a message or execute the
prepared command. A mismatched daemon endpoint or workspace is rejected.

The daemon distinguishes interactive run conversations from governed Workbench
sessions. Chat, status, diff, and candidate controls use the handed-off run.
Queue, context, goals, checkpoints, and other governed controls require selecting
or creating a Workbench session with `/sessions` inside the CLI. These are not
silently inferred from an unrelated run ID; the console explains that selection.

The embedded Xterm terminal accepts actual keyboard/mouse input and has visible
buttons for navigation keys, Enter, Escape, and other terminal controls. Closing
the panel retains the process for reopening after browser reload using `/consoles`. **Terminate
console** terminates it. Advanced flows retain Peritus's own prompts and checks.

## Scriptable CLI

`/cli` opens an editable command form. `/cli ARGUMENTS…` runs immediately in a
retained console, passing arguments directly to `peritus`. Daemon commands get
the discovered/overridden endpoint; launcher/setup/help/completion commands use
their ordinary CLI entry points. The command is not executed through a shell.

Mouse-selectable templates cover:

- Daemon status and graceful shutdown.
- Artifact get, put, and transfer cancellation.
- Event subscriptions.
- Prompt answer and cancellation.
- Terminal attach, input, resize, detach, and cancellation.
- Command-envelope submission with explicit actor and idempotency key.
- Shell completions and update-check configuration.

Template `ID`, `PATH`, `TOPIC`, and `KEY` values are placeholders. Replace them
with the exact values required by the installed CLI. Other ordinary CLI
subcommands can be entered in the same form. `/cli --help` opens the installed
version's actual help; its command grammar is the authority for advanced flags.

## Verification scope

Native Git and workspace/file/configuration paths have integration coverage.
Native conversations have a negotiated local-socket fixture proving exact
mode, target, provider/model/effort serialization and observation projection.
Regressions cover saved session choices across restart, native run adoption and
model-update recovery, retained terminal rediscovery, exact PTY arguments, and
CLI/TUI run selection. The actual CLI's `--version` is exercised through a PTY. Provider onboarding, inference, and all advanced workflows
have not been exercised end-to-end in this pass.

This implementation offers native common controls plus a working full-CLI
route. It does not implement every workbench capability as a native web panel,
and does not mirror later manual navigation inside a CLI console back into the
browser's selected tab. The initial handoff is bound to the browser session.
