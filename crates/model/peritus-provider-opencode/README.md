# OpenCode model transport

## Folder guide

Official OpenCode model transport for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-opencode`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Official OpenCode CLI transport for Zen/Go, executable/model discovery, isolated config/auth, request encoding, JSONL decoding, and normalized failures. |

Runs the unmodified official OpenCode CLI as a model transport for OpenCode Zen and Go.
Peritus supplies the OS-stored API key to a private, disposable auth file. No separate
OpenCode login is required. Account billing, subscriptions, and model permissions still apply.

Each turn replays Peritus history and its inert tool catalog. A private primary agent has
all native permissions denied and tools disabled. Project config, plugins, MCP, LSP,
formatters, automatic compaction, sharing, and updates are disabled. The child receives a
private vendor configuration directory, including an explicit PWD, isolated XDG state paths,
and a vendor home override. Ambient project ancestors are rejected before any key is written. Peritus
executes validated tool proposals; OpenCode does not operate on the user's workspace.

The adapter parses the official JSONL interface, rejects native tool events and incomplete
or failed executions, reports usage, and retains provider failures as redacted diagnostics.
Public report objects are serialized once, and literal transcript text avoids an extra escaping
layer. Discovery skips unsupported PATH candidates. Native executable resolution is deferred
until this route is used, keeping unrelated provider startup independent.
Model discovery uses `opencode models SERVICE --verbose`, including advertised metadata.
Output-token limits are advisory. Text and Peritus tool history are supported; native session
continuations, image attachments, explicit reasoning controls, and server cancellation are
not currently advertised. Local cancellation terminates the owned process group.

The adapter requires the tested OpenCode 1.19.0 contract because isolation depends on its
configuration semantics. Setup installs that official version. Recognized subscription and
model-access rejections abort the vendor retry loop immediately. Other vendor retries retain
the official CLI behavior inside the bounded invocation. See the real-interface campaign report
for bounded behavioral evidence.
