# Peritus WebUI

A conversation-first, local browser workspace for Peritus. The Svelte client is
served by the `peritus-web` Rust gateway; the existing Peritus daemon owns agent
execution. Multiple projects and nested sessions stay in one browser tab.

**Status: optional and experimental.** This source-built interface is not part
of the native installers and does not replace the CLI/TUI. Linux/Chromium has
focused local coverage; other platforms, browsers, and assistive technologies
still need qualification. Read the [security boundaries](SECURITY.md) before
connecting projects or exposing the service through other software.

The interface uses the user-selected Nixie laboratory control-panel system:
recessed displays, cathode counters, physical keys, mechanical transitions, and
an optional click sound. See [`DESIGN.md`](../DESIGN.md) for the visual system and
[`COMMANDS.md`](COMMANDS.md) for exact native/CLI capability routing.

## Build and start

Prerequisites:

- The Rust toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml).
- Node matching `package.json` and npm, **for frontend development/build only**.
- Git and an installed, compatible `peritus`/`peritusd` for harness workflows.
  See [source installation](../packaging/README.md#install-from-source).

Run from the repository root:

```sh
npm --prefix webui ci
npm --prefix webui run build
cargo build -p peritus-web -j 2
./target/debug/peritus-web --root "/absolute/path/to/project"
```

Open **http://127.0.0.1:4173**. The gateway binds only to IPv4 loopback. Stop it
with Ctrl+C. No Node process or Vite server is needed to run the built UI.

For a distributable build, use `cargo build --release -p peritus-web -j 2`, ship
the resulting binary with `webui/dist`, and pass `--assets` explicitly:

```sh
./peritus-web --assets "/path/to/webui/dist" --root "/path/to/project"
```

The development default for `--assets` points into the checkout at compile
time. The static files are not embedded in the binary, and the WebUI is not yet
integrated into the existing native installers.
Distribute the complete asset directory, including `licenses/`; see
[third-party notices](THIRD-PARTY.md) for bundled fonts and libraries.

### First project setup

1. Open the project's **Set up project** action or **Harness console**. This
   launches the installed `peritus open <project-root>` in a real PTY.
2. Complete the existing provider/workspace setup. Provider login and workspace
   trust use Peritus's own interactive flows.
3. Return to the browser and use **Reconnect** (`/reconnect`) to reload provider,
   workspace, and connection observations.
4. Select Chat, Plan, Review, or Build, choose role models if needed, and send a
   message. The status distinguishes daemon-received input from input observed
   as incorporated into a provider request.

Adding a browser project does not register or trust a daemon workspace. The
setup banner remains until a matching canonical root or selected repository is
found in the daemon's workspace catalog. The native composer uses its own run
identity; the advanced CLI console has its **own visible conversation selection**.

### Development

Run the Rust gateway on port 4173, then, in another terminal:

```sh
npm --prefix webui run dev
```

Open the Vite URL at `http://127.0.0.1:5173`. Its `/api` proxy targets the gateway.
Use the gateway URL to inspect the production build.

## Everyday operation

- **Projects:** use the project rail to switch or add project directories.
  Paths refer to the machine running the gateway. Each project has an **X**;
  `/close-project` closes the selected project too. Closing works for missing
  folders and retains sessions and daemon work. Reopen the same directory with
  **Open project**, or reopen a saved session from **Session library**.
- **Sessions:** create siblings or nested children, drag a session onto another
  session, or use **Organize session** to rename/reparent with the keyboard.
  Cross-root nesting and cycles are rejected server-side. Arrow keys navigate
  session tab rows. Each tab has an **X** to close it, including when its project
  directory has been deleted. Closing preserves its conversation and work;
  reopen it from **Session library**.
- **Files:** click an explorer entry to open a session-local file sub-tab.
  Hidden, ignored, and untracked entries are included. **Load more** retrieves
  the next 250 entries. Code, UTF-8 text, sanitized Markdown, PDFs, images, and audio
  have dedicated viewers. Viewing a file does not attach it to model context.
- **PDFs:** open read-only in the browser's built-in PDF viewer, with its native
  page, zoom, search, and print controls. No PDF-rendering dependency is bundled.
  **Open PDF in new tab** and **Download** remain available outside the frame.
  Browsers without enabled inline PDF support show an explicit fallback;
  malformed, encrypted, or unsupported documents may require a PDF application.
- **Quick editing:** text files open in **View**. Choose **Edit** for small
  Markdown/configuration/source tweaks; use **Save** (Ctrl/Cmd+S), **Revert /
  reload**, literal find/replace (Ctrl/Cmd+F), bounded undo/redo, wrapping, and
  line/column navigation. Tab moves focus. Drafts survive switching file/session
  tabs in the current page; leaving the page warns about unsaved changes. File
  drafts are not crash-persistent. Saves preserve UTF-8 BOM, LF/CRLF, and file
  permissions, and reject a changed disk revision. Mixed line endings and
  symbolic/hard-linked files require an external editor. This is not an IDE;
  no editor dependency, language server, or additional npm package is used.
- **Attach:** drag an explorer file into the session, or choose **Attach to next
  message** in its file-actions menu. A saved UTF-8 snapshot is queued only for
  that native session; remove it before sending if unwanted. Native chat's
  current protocol accepts text, not image/audio payloads: 48 KiB per file,
  at most 16 files, and 64 KiB for the complete encoded message. Oversized input
  is rejected, never truncated. These context limits are separate from preview
  size. Non-text media remain view/download-only here; the separate Workbench
  console has its own attachment selection. Snapshot bytes and hashes are
  stored beside the gateway state, with a 256 MiB aggregate cache bound.
- **Git:** select **Source control**, then configure the repository directory if
  necessary. It may be an existing directory anywhere inside the project root,
  including one without `.git`. Git actions become usable once that location
  resolves to a working repository inside the project. Stage, unstage, inspect
  diffs, commit, pull, and push use actual Git. Pull is fast-forward-only.
  **Branches** provides create/switch, remote tracking, rename, and confirmed
  merged-branch deletion. **Remotes** provides add, edit fetch/push URL, rename,
  remove, fetch/prune, and publish with upstream tracking. No force push,
  forced checkout, or deletion of unmerged branches is exposed.
- **Ignore:** use an explorer entry's context menu to preview a literal,
  root-anchored `.gitignore` rule. Applying it preserves existing content;
  already tracked files remain tracked.
- **Commands:** press **Ctrl/Cmd+K**, click **Commands**, or enter `/help`.
  The mouse directory and composer slash commands call the same dispatcher.

### Default shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd+K | Command directory |
| Ctrl/Cmd+Shift+E | File explorer |
| Ctrl/Cmd+Shift+G | Source control |
| Ctrl/Cmd+Alt+N | New sibling session |
| Ctrl/Cmd+, | Console configuration |
| Enter in the composer | Send message or run command |
| Shift+Enter in the composer | New line |
| Escape | Close the active dialog |

Slash arguments support quoted paths and text. They are parsed as argument
vectors, not shell expressions; `$VARIABLE`, pipes, and substitutions are not
expanded. Examples:

```text
/open "/home/me/Work Projects/example"
/nest
/plan Inspect the current architecture
/git add "src/my module.ts"
/git commit -m "Explain the change"
/cli status
```

`/git commit` commits the selected ordinary repository. `/commit` is the separate
Peritus **candidate** workflow, with the exact run passed to the installed CLI.

## Persistent configuration

**Console configuration** provides visual controls and a TOML editor with
import/export. Save applies validated settings; **Reset to defaults** restores defaults.
Editing the file externally takes effect on reload or `/reconnect`.

| Platform | Configuration root | State root |
| --- | --- | --- |
| Linux/other Unix | `$XDG_CONFIG_HOME/peritus`, otherwise `~/.config/peritus` | `$XDG_STATE_HOME/peritus`, otherwise `~/.local/state/peritus` |
| macOS | `~/Library/Application Support/Peritus/Config` | `~/Library/Application Support/Peritus/State` |
| Windows | `%APPDATA%\Peritus` | `%LOCALAPPDATA%\Peritus\State` |

The preferences file is `<configuration-root>/webui.toml`. Project/repository
bindings, session organization, and original operation records are in
`<state-root>/webui/workspace.json`. Active tabs, file tabs, modes, and unsent
drafts are browser-local, under `peritus:layout:v1`; changing browser origin or
clearing site data gives that browser a fresh presentation state. Daemon-owned
conversations remain in Peritus's storage.

Example dotfile (aliases and color overrides here are optional customizations):

```toml
theme = "nixie"                 # nixie | daylight | blueprint
density = "comfortable"         # comfortable | compact
motion = true                   # OS reduced-motion preference still wins
sound = false
font_size = 14                  # 12–22
font_family = "Barlow, sans-serif"
mono_family = "Iosevka, monospace"
explorer_width = 248            # 180–480
controls_visible = true
explorer_visible = true
word_wrap = true
markdown_preview = true

[shortcuts]
commands = "Mod+k"
files = "Mod+Shift+e"
git = "Mod+Shift+g"
new = "Mod+Alt+n"
settings = "Mod+,"

[aliases]
branch = "/nest"
changes = "/git diff"

[tokens]
accent = "#efa466"
```

`Mod` means Ctrl or Command. Shortcut keys name dispatcher commands, with
`commands` reserved for the directory. Alias names use lowercase letters and
hyphens; values must be single-line slash commands. At most 100 shortcuts and
100 aliases are allowed, and alias recursion is bounded. Missing top-level
fields use defaults; supplied maps replace their default map. Unknown top-level
fields and invalid settings are rejected. Color override roles are
`background`, `panel`, `display`, `text`, `muted`, `accent`, and `line`, using
`#RGB` or `#RRGGBB`. Custom palettes should retain readable contrast.

Barlow, Barlow Condensed, and Rajdhani are bundled locally. Other configured
fonts, such as Iosevka, use the system installation or their fallback stack.

### Gateway options

| Option | Meaning |
| --- | --- |
| `--root PATH` | Initial project when the workspace state is empty; defaults to the current directory |
| `--port N` | Loopback HTTP port; default 4173 |
| `--assets PATH` | Built static frontend directory |
| `--config PATH` | WebUI TOML preferences file |
| `--state PATH` | WebUI workspace/operation JSON file |
| `--daemon-config PATH` | Exact generated Peritus daemon configuration; default discovers the latest native generation |
| `--product-state DIR` | Peritus product-state generation directory; defaults to the native state root's `product-state` |
| `--endpoint PATH` | Explicit daemon socket/named-pipe endpoint; otherwise derived from daemon configuration |
| `--cli PATH` | Installed Peritus executable; default `$PERITUS_BIN` or `peritus` on PATH |

Overriding WebUI configuration/state files does not redirect daemon discovery.
For an isolated daemon, supply its daemon configuration, endpoint, and
product-state directory explicitly.

## Execution and recovery

The browser is a presentation client. Native conversations use the existing
negotiated application client over the daemon's local socket. The gateway does
not contain another agent, provider runner, or policy engine.

Mutations receive an original operation ID recorded before dispatch. Reusing
an ID with different input is rejected; a repeated identical request returns
its original outcome. On connection failure the client queries that operation
instead of blindly resending. If an outcome remains uncertain, inspect the
named run or repository before submitting a new action. These gateway records
complement the daemon's own receipts; they cannot turn an interrupted Git or
CLI process into a known successful result.

Explorer edits, ordinary Git actions, and retained CLI processes are explicit
local-user operations under the gateway account's OS permissions. They are not
agent tool calls and do not pass through the daemon's agent approval/sandbox
pipeline. Daemon workspace trust gates native agent execution, not these local
controls. This distinction is part of the experimental interface's security model.

Connected is not the same as ready: structured daemon readiness and local
Doctor checks must confirm the workspace/provider route before sending.
No provider authentication/network probe is run implicitly. A read-only or
draining daemon remains visible but cannot accept new messages.

The gateway durably retains the exact native request before transmission and
its exact response when observed. Interrupted requests stay unresolved instead
of being recorded as definite failures. The recovery banner queries the original
record and blocks another mutation to its target. Native `Interact` does not yet
expose an authoritative daemon receipt-query endpoint: a lost daemon response
therefore requires inspecting the conversation and explicitly acknowledging the
outcome. That acknowledgement clears the hold; it never resends or labels the
old action successful. Reconnection renews a rotated gateway token automatically.

CLI consoles retain processes while their panel is closed and can be reopened
from the current page. **Terminate console** terminates the CLI process. Console IDs
are page-local, output is held in server memory (1 MiB per console, maximum 24
consoles), and gateway shutdown ends those PTYs. Closing a browser session tab
does not cancel daemon-owned work.

File reads reject canonical paths outside the selected project, including
outward symlinks. Text previews require UTF-8 and are limited to 50 MiB; downloads
and browser media use raw streaming with byte-range support. Unsupported media
has a download route. The gateway uses loopback host/origin checks, same-site
cookies, a per-process API token, and a content security policy for the local
single-user deployment.

PDF previews bypass UTF-8 decoding. A small header check gates the PDF-only
inline response, which forces `application/pdf` and permits same-origin framing.
The browser's PDF engine performs document parsing; the header check is not a
malware scanner or complete format validation. Other raw files retain their
sandbox policy. All routes retain authentication, canonical path confinement,
`nosniff`, and byte-range delivery. Keep the browser updated for PDF-engine fixes.

Large text previews use bounded plain-text pages, with complete file access;
they do not create a highlighted/Markdown DOM for the entire file. Editing
files over 2 MiB requires a visible performance confirmation, not a reduced
preview limit. The text-save endpoint accepts up to 50 MiB separately from
the normal 4 MiB action-body limit, and journals hashes rather than file bodies.

## Focused validation

From the repository root:

```sh
cargo test -p peritus-web -j 2
cargo clippy --locked -p peritus-web --all-targets --no-deps -j 2 -- -D warnings
npm --prefix webui run check
npm --prefix webui run test:unit
npm --prefix webui run build
cargo build -p peritus-web -j 2
npm --prefix webui run test:e2e
```

The browser tests need Playwright Chromium (`npx playwright install chromium`
from `webui` if absent), Git, and the installed Peritus CLI. They own port 4174,
use temporary fixtures under `/tmp/opencode`, and isolate daemon discovery.
Their real Git remote is a temporary local bare repository.
PDF tests use the full Chromium channel and generated two-page documents,
checking actual native-renderer completion as well as HTTP delivery. Installing
only the headless shell (`--only-shell`) is insufficient for these tests.

The [Browser workspace workflow](../.github/workflows/webui.yml) runs these
frontend/gateway checks and builds the real CLI for browser integration on an
Ubuntu runner. The existing workspace CI retains its broader Rust/platform
coverage. Hosted CI has not been run from this local implementation session.
The exact frontend npm scripts and runner commands are registered in
`xtask/src/reproducibility/workflow_webui.rs`; intentional script changes need a
matching policy update. The reproducibility test suite covers that boundary.

Verified locally on Linux/Chromium:

- Seventeen Rust tests, including bounded binary-safe PDF identification,
  canonical path/nesting constraints, exact
  50 MiB preview boundaries, safe saves, real Git workflows, immutable text
  attachments, read-only readiness, and lost-response/restart recovery against
  a negotiated socket fixture with exact native targets and revisions.
- Sixteen frontend unit tests and nineteen integration/browser tests covering
  command routing, text formats, project isolation, original operation outcomes,
  real Git round trips, editing/conflicts, drag/drop attachments, gateway restart,
  `.gitignore`, file tabs, drafts, configuration, and a real CLI PTY invocation.
  PDF checks cover readable native rendering, unsupported-browser fallback,
  invalid-header recovery, switching away from pending text, and delivery security.
- Svelte/TypeScript check without errors or warnings; desktop/phone captures
  without page errors, horizontal overflow, or automated axe WCAG 2.0 A/AA
  violations.

Live model inference, provider login, every advanced workbench flow, screen
readers, Firefox/Safari, and non-Linux gateway execution have not been verified
in this implementation pass. The automated accessibility results do not alone
establish WCAG conformance. Full native-panel parity is not claimed: advanced
capabilities run in the embedded real CLI as documented in
[`COMMANDS.md`](COMMANDS.md).

## Code map

- `src/App.svelte`, `src/app.css`: console composition and responsive visual system.
- `src/lib/components/`: viewers, explorer, Git, models, settings, terminal, and dialogs.
- `src/lib/workspace.svelte.ts`: reactive presentation state and shared dispatcher.
- `src/lib/commands/`: command registry and side-effect-free slash parser.
- `src/lib/api.ts`: HTTP transport and original-operation reconciliation.
- `../crates/app/peritus-web/src/`: HTTP, state, files, Git, daemon adapter, and PTY modules.
- `tests/`: focused parser and integration coverage.

Add commands through the registry and dispatcher, then implement their native
gateway route or an explicit CLI handoff. Keep viewing separate from context
attachment, and preserve project/session/run identities across every route.
