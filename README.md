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

## Real testing

Peritus uses the [real-testing skill](skills/real-testing/SKILL.md) to build and run the actual application, exercise its human interfaces, and retain reproducible defects and replay evidence. We use this approach based on the findings in [Rethinking the Value of Agent-Generated Tests for LLM-Based Software Engineering Agents](https://arxiv.org/html/2602.07900v1). The paper reports limited task-outcome gains from agent-generated tests in the studied settings, with additional token and API costs; our choice of behavioral testing is a project policy informed by those findings.

All bundled guidance lives in [skills/](skills/README.md). Adding a skill folder and its resources automatically includes them in the next build; agents discover them through `skill_list` and read them through `skill_read`.

Designer, writer, reviewer, and fixer prompts include brief planning, careful execution, evidence, and root-cause repair cues. Rust work also activates the bundled [Rust reliability skill](skills/rust-reliability/SKILL.md); its guidance stays in the current system policy and is reinforced when unchanged inspection repeats. References are available to the model through read-only skill access. User restrictions on commands and tests still apply, and the optional check helper is never an automatic gate.

Repository unit suites, external qualification gates, and formal proof tooling have been retired. Peritus's design, check, reviewer, and fixer phases remain part of the application. Earlier technical records are retained as history.

Peritus can exercise browser pages and Linux desktop apps during coding runs. Native installers
automatically provision Python, Playwright, and Chromium on Linux, macOS, and Windows; portable
and system-package installations prepare the per-user runtime on first interactive launch.
Use `peritus automation setup` to repair it, and inspect availability with
`peritus automation status`. The model receives real screenshots alongside DOM or native accessible
controls, and keeps sessions alive between actions. It can launch fresh browser profiles and isolated
X11 desktops, or attach to an explicitly selected local Chrome DevTools endpoint or native app PID.
See [computer automation](docs/computer-automation.md) for setup, capabilities, and platform limits.

Peritus also has native web page reading and keyless search in Chat, Design, Review and coding runs.
Search is built into Peritus in Rust and works without setup, API keys, Python or containers.
An existing SearXNG server can optionally be selected with `peritus web setup URL`.
Known URLs can be read without a search backend. The model can page/search retained sources and use
its isolated browser for JavaScript content. See [web tools](docs/web-tools.md).

Input-token profiles guide automatic compaction. Peritus imposes no fixed local 200,000-token
input ceiling; the selected provider enforces its actual context capacity. Document reads
default to 1 MiB and can request multi-megabyte pages up to protocol capacity. Pages that
fit the selected transport reach the model intact. When a result exceeds Codex's separate
submitted-turn character limit, Peritus retains the full observation locally and supplies
an explicitly bounded view with exact retrieval handles. Large stopped continuations are
saved separately from their small run snapshots so history can survive restart.

## Install

Install the latest public release with the command for your platform below. To select an older
release or install from source, see the [installation instructions](packaging/README.md).

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

To reopen the most recently active saved conversation for the current folder, run:

```sh
peritus resume
```

This resolves the folder through the normal workspace setup and opens the conversation without
starting new work. If the folder has no saved conversation, Peritus opens a new conversation and
says so.

## Experimental browser workspace

The [harness improvement inbox](docs/harness-improvements.md) collects evidence-backed suggestions for explicit later patch generation and evaluation. Open `/improvements` in the WebUI or use `peritus improvements` in the CLI.

An optional [WebUI](webui/README.md) provides project/session tabs, file previews
and small text edits, context attachments, Git controls, and native conversations.
It uses the existing Peritus daemon; the terminal interface remains available.

The WebUI currently requires a source build and is not included in the standard
installers. Node is needed to build the frontend, not to run the built application.
It is intended for one local user, not remote hosting or multi-user access.
See its [security boundaries](webui/SECURITY.md) and
[native versus CLI capability map](webui/COMMANDS.md) before use. Full CLI parity,
cross-platform qualification, and complete accessibility acceptance are not claimed.

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
| `/design [idea]` | Create a design together: one question at a time, suggested answers, and a saved Markdown draft. |
| `/design show` | Show the current draft. Reply with changes or paste replacement Markdown to revise it. |
| `/design build` | Explicitly start implementation using this conversation's saved design. |
| `/plan` or `/review` | Discuss a plan or perform an independent review with read-only tools, including an exact absolute reference path named in your request. |
| `/build <request>` | Start checked writer, reviewer, and fixer delivery. |
| `/model` | Discover provider models; arrows and Enter select, Tab switches roles. |
| `/effort` | Select per-role reasoning effort; also press `e` in the model picker. |
| `/new` | Start another conversation without deleting prior work. |
| `/status`, `/diff`, `/details` | Inspect progress, changes, and public tool summaries. |
| `/stop` | Stop the current work and preserve effects already completed. |
| `/runs` | Open the run and candidate dashboard. |

Design mode helps you work out the goal, users, important journeys, scope, approach, and
acceptance through real use. Say “not sure” to leave a decision open, or change an earlier
answer at any time. The draft and next question are saved with the conversation, so you can
return after closing the client or restarting the daemon. The Markdown artifact lives beside
the run trace as `<run-id>.guided-design.md`; Peritus shows its path when the design is ready.
Design mode can inspect the project but cannot edit its files or run commands. Approving a
design keeps you in the interview; `/design build` is the separate implementation action.
The browser's **Design** mode offers **Show draft**, **Revise draft**, and **Start implementation**.

Use Shift+Enter for a new line. The mouse wheel or PageUp/PageDown scrolls the conversation.
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
check coverage or an interrupted review leaves effects in place with completion still unconfirmed.
Commands run with your local user permissions. There is no automatic rollback of in-place changes.
`/diff` shows the available comparison evidence. `/build` and candidate actions such as `/commit`
and `/discard` require the managed Git workflow; they are not offered for in-place delivery.

In `/runs`, select a task before using these dashboard keys:

| Key | Action |
| --- | --- |
| Enter | Open the run's durable conversation. |
| `m` | Send a follow-up through the durable workbench conversation. |
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

API-key choices include OpenAI, Anthropic, Google Gemini, OpenCode Zen and Go, OpenRouter,
Groq, Together AI, Fireworks AI, DeepSeek, and an explicitly configured compatible endpoint.
In provider settings, press `t` on an existing direct provider to test generation, tool calling,
and a tool-result round trip. The optional test makes up to three small requests and may use
paid tokens; saving a key or listing models does not run it. See [provider contracts](docs/provider-contracts.md)
for API sources, discovery behavior, and test limits.

Zen and Go run through the official OpenCode CLI with its native tools disabled. Setup offers
to install the CLI and uses the key stored by Peritus; a separate OpenCode login is unnecessary.
OpenCode selects the model's provider protocol. Peritus owns tools and conversation history.
Your key's model permissions and billing still apply; Go also requires an active Go subscription.

Setup and `/model` query provider-advertised model catalogs. No built-in model list is substituted
when discovery fails; `/model manual MODEL_ID` is an explicit manual fallback.
Model changes apply at an idle boundary. Interactive selections never silently fail over.
Automatic coding-run provider failover requires permission in provider settings and is off by default.

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

- A terminal connection failure reconnects automatically with bounded backoff. The current
  conversation, selected models, and unsent chat text stay in the interface. Use Ctrl-R or
  `/reconnect` for immediate recovery, including restoring daemon readiness.
- If a provider login fails, run `peritus providers`.
- If a workspace needs repair, run `peritus workspaces`.
- Interrupted provider responses retry automatically within the task's limits. After a daemon
  restart, interrupted tasks continue from preserved work. Tasks you explicitly cancel stay stopped.
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
The private bounded `logs/peritus-tui.log` records terminal connection failures and reconnection
generations without recording messages or drafts.
Keep credentials and private repository content out of public bug reports.

For context inspection and recovery, see [Local working memory](docs/local-working-memory.md).
To remove Peritus, see [Uninstall](packaging/README.md#uninstall). Removal keeps your task state and credentials.

## Get help

Run `peritus --help` for command help.
Use [GitHub Issues](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues) to report a problem.
Include your operating system, Peritus version, and the steps that reproduce the problem.

## Codebase navigation

Start with the [repository agent map](AGENTS.md) to choose an implementation owner. Every repository-managed directory has an `AGENTS.md` waypoint describing local files, child responsibilities, and the route back to its parent/package. Layer and package READMEs provide human background and usage. See [navigation coverage and scope](docs/navigation-map.md) for the documented boundary.

### Native Rust plugins and modules

Open **Modules** in the browser or `/modules` in the terminal to discover, install,
enable, and select native Rust plugins from the official market:
https://github.com/Corvidae-Coding-Projects/Peritus-Module-Market

Plugins contain independently compiled Rust code and can add their own commands,
agent tools, persistent state, and menu actions without rebuilding Peritus. Deep
Research's Rust plugin prepares an approved plan, gathers primary sources, applies
skeptical admission, synthesizes a report, and fact-checks that exact report. Its
feature code lives in the market repository; Peritus supplies shared host services.

Deep Research also has a dedicated live controversy researcher and a report section for
specific accusations, criticism, responses, and documented outcomes. Optional **Sicko mode**
continues research with visible checked-report checkpoints and explicit steer, pause, resume,
and normal-mode controls. It has no cumulative token/time/round budget. The v1.4 plugin
requires the host's continuing-checkpoint service; ordinary research remains the default.

Use `/module ID COMMAND_OR_TOPIC`. Updates install disabled. Native code runs under
your OS account; agent access to host tools follows current permissions. Local
packages remain installable with `peritus modules install DIRECTORY`. Existing
bundled Source brief and declarative workflows remain available. See the
[module guide](crates/orchestration/peritus-modules/README.md) for controls and the
initial platform boundary.
