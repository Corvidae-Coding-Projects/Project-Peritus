# Peritus

Peritus is a coding agent that runs in your terminal. It changes code, runs project checks, and reviews the result.
You can start in any folder. In a plain folder, requested edits happen in place.
Git repositories use a separate managed copy. You control which changes to keep.

## HarnessBench: 85.46% combined across all 106 tasks

**Our frozen local run scored above the published HarnessBench leaderboard leader.**
It scored **85.46% combined** and **88.56% outcome**, with all 106 tasks included and
adverse results retained in the score.

| Configuration | Combined | Outcome / completion |
| --- | ---: | ---: |
| Peritus — frozen local run, GPT-5.6-sol + Sonnet | **85.46%** | **88.56%** |
| NanoBot + GPT-5.4 — published combined-score leader | 81.3% | 85.1% |
| Codex + GPT-5.4 — published reference | 80.4% | 86.5% |

Published figures were checked on the [HarnessBench leaderboard](https://www.harness-bench.ai/leaderboard.html)
on September 6, 2026. Peritus's recorded combined score is 4.16 percentage points higher than
the published leader's displayed score.

**This is a score comparison, not an official leaderboard placement or a controlled same-model win.**
Peritus used a mixed-model configuration and GPT-5.6-sol as its process judge; the
[published evaluation](https://arxiv.org/html/2605.27922v1) used Claude Sonnet 4.6 as its process judge.
Our result is one retained scored execution per task, not a repeated reliability estimate.
It does not establish which harness would win with identical models and evaluation conditions.

The run used frozen Peritus revision `a819e58244175711eba1f9c8baaab8ac8b92357c` and
HarnessBench revision `1025086a446653702b80cfb48babbeec35db6b2c`. Process scored 95.97%
and security scored 100%; 41 tasks had perfect external outcomes. Native acceptance succeeded
on 98 tasks; eight retained unaccepted candidates. All remain in the aggregate.
The retained aggregate's SHA-256 is `88cdb94169783031dcd0e536a38ad0e2ebf84cc5f17e5cfe0467d470c1cffdaf`.
See the [benchmark integrity policy and findings](docs/benchmark-integrity-appendix.md)
for how we preserve failures and distinguish product defects from evaluator problems.

## Install

No public release is available yet. The commands below will work after the first release is published.
For now, use the [source installation instructions](packaging/README.md#install-from-source).

### Linux and macOS

Run this command in a terminal:

```sh
curl -fsSL https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/latest/download/install.sh | sh
```

### Windows

Run this command in PowerShell:

```powershell
irm https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/latest/download/install.ps1 | iex
```

The installer selects the package for your operating system and processor. Packages support x86-64 and ARM64 processors.
It checks the download and installs Peritus for your user account.
It also installs missing runtime dependencies. Your operating system can ask for administrator approval to install these dependencies.

Open a new terminal after installation. To check the installation, run:

```sh
peritus --version
```

For system requirements, manual downloads, and installation options, see [Installation](packaging/README.md).

## Start

1. Open a terminal in the folder where you want to work. Git is not required.
2. Run `peritus`.
3. Select a model provider.
4. Complete the login steps.
5. Before you give Peritus permission to edit files or run commands, check the folder path.

Peritus starts its local background process automatically. You do not need to write a configuration file.

You can use OpenAI, Anthropic, Gemini, or a compatible API service.
ChatGPT and Claude account connections use the separate `codex` and `claude` tools.
If the selected tool is missing, setup asks before it downloads and runs the provider's installer.
Model providers can charge for use. An installation does not include a provider account or model weights.

Install the build tools that your project needs. Peritus cannot check a project without its test and build commands.

To open a different folder, run:

```sh
peritus open /path/to/folder
```

## Start a conversation

Type a question, discuss an idea, or request a change, then press Enter.
Ordinary conversation does not automatically start a build. The composer stays available while
Peritus works; follow-up messages can correct or redirect it. The status distinguishes input
received by the daemon from input incorporated into a model request.

When you ask Chat to implement or fix something, it hands the request to the existing design,
writer, exact-target checks, independent reviewer, and fixer pipeline. Questions, diagnosis,
planning, and read-only review remain conversational; they do not authorize edits.

The conversation shows your messages and the model's replies, with one `*working (40s)` indicator
while work is active. Harness status messages and tool activity stay behind `/details`; failures
remain visible. The timer measures elapsed time since this client observed the current busy period,
not evidence of model progress.
The account-backed Codex adapter delivers complete messages, not token-by-token text.

Type `/` to discover commands; Tab completes them.

| Command | Action |
| --- | --- |
| `/plan` or `/review` | Discuss a plan or perform an independent review with read-only tools. |
| `/build <request>` | Start checked writer, reviewer, and fixer delivery. |
| `/model` | Discover provider models; arrows and Enter select, Tab switches roles. |
| `/effort` | Select per-role reasoning effort; also press `e` in the model picker. |
| `/new` | Start another conversation without deleting prior work. |
| `/status`, `/diff`, `/details` | Inspect progress, changes, and public tool summaries. |
| `/stop` | Stop the current work and preserve effects already completed. |
| `/runs` | Open the run and candidate dashboard. |

Use Shift+Enter for a new line and PageUp/PageDown to scroll.
In the message composer, Ctrl+Left/Right moves by word. Hold Shift with Left/Right,
Ctrl+Left/Right, or Home/End to select text; typing, pasting, Backspace, and Delete
replace or remove the selection. Escape clears it. Click to position the cursor,
or drag or Shift-click to select. Mouse input requires terminal mouse reporting;
use your terminal’s selection override (usually Shift-drag) to copy screen text.
Diff and check reports also support PageUp/PageDown; Home returns to the start.
Ctrl+C closes the interface when idle. During active work it requests a stop; press it again to
close without waiting. Ctrl+Q closes the interface without cancelling
daemon-owned work. Conversations and task state remain available between sessions.

Plain folders do not need `git init` or an initial commit. You can chat and inspect files before
trusting the folder. After trust, ask for changes or commands in `/chat`; edits happen in that folder.
The same pipeline checks individually tracked task files without a whole-folder snapshot. Missing
verification coverage or an interrupted review leaves effects in place but not verified complete.
Commands run with your local user permissions. There is no automatic rollback of in-place changes.
`/diff` shows the available comparison evidence. `/build` and candidate actions such as `/commit`
and `/discard` require the managed Git workflow; they are not offered for in-place delivery.

In `/runs`, select a task before using these dashboard keys:

| Key | Action |
| --- | --- |
| Enter | Open the conversation; older runs use their existing message composer. |
| `m` | Send a follow-up through the legacy task composer. |
| `i` | Inspect the result. |
| `a` | Accept the result. |
| `c` | Commit the changed files. |
| `p` | Export the changes as a patch. |
| `x` | Cancel the task. |
| `r` | Retry a failed or cancelled task. |
| `D` | Discard the task's changed files. |
| `?` | Show all keyboard commands. |

**CAUTION:** `D` removes the task's changes. Before you discard changes, export the changes that you need.

A task can stop with useful changes but incomplete checks. Before you accept or commit that result, read the remaining work.
For more controls, see the [product guide](docs/g4-product-experience.md).

## Change settings

```sh
peritus providers
peritus workspaces
```

Use `peritus providers` to add a provider, change a provider, or repair a login.
Use `peritus workspaces` to select a repository or repair its managed copy.

Setup and `/model` query provider-advertised model catalogs. No built-in model list is substituted
when discovery fails; `/model manual MODEL_ID` is an explicit, unverified fallback.
Model changes apply at an idle boundary. Interactive selections never silently fail over.
Legacy coding-run provider failover requires permission in provider settings and is off by default.

## Update

To install an available update, run:

```sh
peritus update
```

Peritus also checks for updates when it starts, at most once every six hours.
A failed update check does not prevent offline use.

To stop automatic update checks, run:

```sh
peritus update --disable-checks
```

To start automatic update checks again, run:

```sh
peritus update --enable-checks
```

## Recover from a problem

- If the connection to the background process fails, use `/reconnect`; the current conversation,
  selected models, and unsent chat text stay in the interface while daemon readiness is restored.
- If a provider login fails, run `peritus providers`.
- If a workspace needs repair, run `peritus workspaces`.
- If a task stops, read its remaining work. Send a message to continue it, or select it in `/runs` and press `r` to retry.

Do not delete the state directory to repair a task. It contains task history and managed repository copies.

The default state directories are:

| Operating system | Directory |
| --- | --- |
| Linux | `~/.local/state/peritus` |
| macOS | `~/Library/Application Support/Peritus/State` |
| Windows | `%LOCALAPPDATA%\Peritus\State` |

Linux uses `$XDG_STATE_HOME/peritus` when `XDG_STATE_HOME` is set.
The `logs/peritusd.log` file in the state directory contains background-process diagnostics.
Keep credentials and private repository content out of public bug reports.

For context inspection and recovery, see [Local working memory](docs/local-working-memory.md).
To remove Peritus, see [Uninstall](packaging/README.md#uninstall). Removal keeps your task state and credentials.

## Get help

Run `peritus --help` for command help.
Use [GitHub Issues](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues) to report a problem.
Include your operating system, Peritus version, and the steps that reproduce the problem.
