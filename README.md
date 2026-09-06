# Peritus

Peritus is a coding agent that runs in your terminal. It changes code, runs project checks, and reviews the result.
It works in a separate copy of your Git repository. You control which changes to keep.

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

1. Open a terminal in your Git repository.
2. Run `peritus`.
3. Select a model provider.
4. Complete the login steps.
5. Before you give Peritus permission to run commands, check the repository path.

Peritus starts its local background process automatically. You do not need to write a configuration file.

You can use OpenAI, Anthropic, Gemini, or a compatible API service.
ChatGPT and Claude account connections use the separate `codex` and `claude` tools.
If the selected tool is missing, setup asks before it downloads and runs the provider's installer.
Model providers can charge for use. An installation does not include a provider account or model weights.

Install the build tools that your project needs. Peritus cannot check a project without its test and build commands.

To open a different repository, run:

```sh
peritus open /path/to/repository
```

## Do a task

1. Press `n`.
2. Describe the result you want.
3. Press Enter to start the task.
4. Read the result.
5. Before you keep the changes, inspect the changed files.

Use Shift+Enter to add a line to a message.
Peritus keeps task files and progress between sessions.
Press Ctrl+Q to close the interface. Closing the interface does not cancel the task.

Select a task before you use these keys:

| Key | Action |
| --- | --- |
| Enter or `m` | Open the task conversation. |
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

Provider changes during a failure require your permission in provider settings. This option is off by default.

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

- If the connection to the background process fails, press `R` to reconnect.
- If a provider login fails, run `peritus providers`.
- If a workspace needs repair, run `peritus workspaces`.
- If a task stops, read its remaining work. Send a message to continue it. To retry it, press `r`.

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
