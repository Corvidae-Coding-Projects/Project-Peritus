# Peritus

Peritus is a local coding agent with a terminal interface, a scriptable CLI, and an optional browser workspace. It helps you discuss changes, design an implementation, edit code, run permitted project commands, and review the result.

Git repositories use a separate managed copy so you can inspect, export, accept, or discard a candidate. Plain folders support edits in place after you grant trust. Conversations, task progress, and completed effects are saved between sessions.

Development version: **0.0.6**. See the [release notes](CHANGELOG.md).

## Install

Install the latest published release for your platform.

### Linux and macOS

```sh
curl -fsSL https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/latest/download/install.sh | sh
```

### Windows

```powershell
irm https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/latest/download/install.ps1 | iex
```

The installers select the package for your operating system and processor, check its download checksum, and install for your user account. Open a new terminal, then run:

```sh
peritus --version
```

## Start working

1. Open a terminal in the folder where you want to work.
2. Run `peritus`.
3. Select a provider and complete its login or API setup.
4. Check the selected folder and its permissions before requesting edits or commands.
5. Discuss your task or ask Peritus to implement a change.

Peritus starts its local daemon automatically. Providers include OpenAI, Anthropic, Google Gemini, and compatible API services. ChatGPT and Claude account connections use the separate `codex` and `claude` tools. Provider accounts and usage costs are separate from Peritus.

To open another folder or reopen its most recent saved conversation:

```sh
peritus open /path/to/folder
peritus resume
```

Ordinary questions and planning remain conversational. Coding requests use the design, writer, project checks, independent reviewer, and fixer pipeline. Task permissions and explicit restrictions determine which commands may run. If checks or review cannot finish, useful edits can remain available with their remaining work clearly recorded.

## Terminal controls

Type `/` to discover commands; Tab completes them. The composer remains available while Peritus works, and follow-up messages can redirect the task.

| Command | Action |
| --- | --- |
| `/plan`, `/review` | Discuss a plan or request a read-only review. |
| `/build <request>` | Start a coding task in the managed Git workflow. |
| `/model`, `/effort` | Choose models and reasoning effort for each role. |
| `/new`, `/sessions` | Start or reopen durable conversations. |
| `/status`, `/diff`, `/details` | Inspect progress, changes, and tool activity. |
| `/files <path>` | Preview a file before adding its content to context. |
| `/checkpoint`, `/rewind` | Inspect saved checkpoints and restore covered files. |
| `/goal`, `/resume` | Inspect a persistent goal or resume its work. |
| `/preview` | Inspect or operate the task's real program preview. |
| `/stop` | Stop active work while preserving completed effects. |
| `/runs` | Open the run and candidate dashboard. |

Use Shift+Enter for a new line. Pasted multiline text stays in the composer until you submit it. Ctrl+Q closes the interface while daemon-owned work continues. Ctrl+C requests a stop during active work and closes the interface when idle.

In `/runs`, select a task before using its actions. Enter opens its conversation, `i` inspects the result, `p` exports a patch, and `v` runs its saved launch instructions. Candidate acceptance and commit actions require the appropriate completed checks and review. Read the remaining work before using a retained candidate. Export changes you need before discarding them.

Plain-folder changes happen in place with your local user permissions. Inspect `/diff` and checkpoint coverage before a restore; completed changes are not automatically rolled back when work stops.

## Browser workspace

The optional WebUI provides project and session tabs, file previews and text editing, context attachments, Git controls, terminals, and durable conversations through the existing daemon. It is intended for one local user. The terminal interface remains available.

Build the frontend and gateway from source, then launch the gateway with the folder you want to open:

```sh
npm --prefix webui ci
npm --prefix webui run build
cargo build --locked --release --workspace
./target/release/peritus-web --root /path/to/folder --assets webui/dist
```

Open `http://127.0.0.1:4173/`. Use `./target/release/peritus-web --help` for explicit configuration, daemon endpoint, state, and port options.

## Real testing

For development, we use the **real-testing skill**, based on the findings in [Rethinking the Value of Agent-Generated Tests for LLM-Based Software Engineering Agents](https://arxiv.org/html/2602.07900v1).

The paper studies agent-generated tests during software issue resolution. It finds that changing test-writing behavior often has modest effects on resolution outcomes while substantially changing interaction and token costs. Our development workflow applies that evidence by focusing on observable program behavior:

- Build and run the actual changed program.
- Exercise its real CLI, terminal, browser, service, and background-job interfaces.
- Define expected results independently of the implementation.
- Explore varied inputs, action sequences, failure paths, and restart recovery.
- Preserve reproducible failures and replay them after repairs.
- Record what was exercised and the remaining coverage limits.

This is the development testing policy. Peritus retains its automatic design, checks, reviewer, and fixer features for the projects it works on.

## Build from source

The repository pins Rust **1.97.1** in `rust-toolchain.toml`.

```sh
cargo build --locked --release --workspace
./target/release/peritus
```

The optional frontend requires a Node version supported by [webui/package.json](webui/package.json). Install the build tools and runtime dependencies needed by the projects you ask Peritus to work on.

## Settings and recovery

```sh
peritus providers
peritus workspaces
peritus update
```

Provider settings manage accounts and endpoints. Workspace settings select or repair managed copies. `peritus update` installs an available published update; `--disable-checks` and `--enable-checks` control automatic update checks.

If the daemon connection fails, use `/reconnect`; the current conversation and unsent draft are retained. Inspect a stopped task's remaining work before resuming it. Explicitly canceled goals stay canceled, and their completed effects and history remain available.

Task state is stored in these locations:

| Platform | Default state directory |
| --- | --- |
| Linux | `~/.local/state/peritus` |
| macOS | `~/Library/Application Support/Peritus/State` |
| Windows | `%LOCALAPPDATA%\Peritus\State` |

Linux honors `$XDG_STATE_HOME/peritus`. Keep the state directory when diagnosing a problem; it contains saved conversations and managed copies. The `logs/peritusd.log` file contains daemon diagnostics.

## Help and license

Run `peritus --help` for CLI help. Report problems through [GitHub Issues](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues), including your operating system, version, and reproduction steps. Keep credentials and private repository content out of public reports.

Peritus is distributed under the [MIT license](LICENSE).
