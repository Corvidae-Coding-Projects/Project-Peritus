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
After a complete successful vendor execution, a malformed model-authored envelope gets one
fresh inference with a correction prompt. Rejected proposals never reach host tools; accepted
corrections account for both inferences and retain a fixed diagnostic audit event. Persistent
format failures identify the rejected part and allow ordinary safe-request recovery. Native
framing errors, process failures, access denials, and cancellation do not enter that correction
path. The wire and correction instructions explicitly reject DSML, XML/HTML-like tool tags, and
special-token pseudo-tool syntax. A complete terminal DSML calls block is also normalized under a
private audit into the same inert tool-call shape, then subjected to the ordinary declared-name,
argument-object, count, and selection checks; incomplete or ambiguous markup still fails closed.
Structural markers are recognized at tag boundaries without rewriting parameter bodies. JSON-valued
parameters pass duplicate-rejecting canonical parsing before flattening, and a structured or fenced
prelude cannot compete with the terminal calls block. String values retain their exact bytes; literal
matching parameter-closing delimiters require the ordinary JSON envelope instead of DSML.
Tool-only turns may omit public commentary without inventing assistant text.
Go regional-privacy rejections name the required account setting instead of suggesting an API
protocol change; Peritus does not modify that setting.
Public report objects are serialized once, and literal transcript text avoids an extra escaping
layer. Discovery skips unsupported PATH candidates, including a Windows installation reached
through WSL interop, which cannot honour the private POSIX configuration, state, and temporary
paths this transport owns. Native executable resolution is deferred until this route is used,
keeping unrelated provider startup independent.
Model discovery uses `opencode models SERVICE --verbose`, including advertised metadata.
Numeric capacities are resolved from the exact service/model before context selection and
generation. Shared context and separate input/output limits retain their distinct meanings.
The base profile remains the route/capability contract and supplies missing-field fallbacks.
Use `request_profile` after `prepare_context` when constructing requests directly; configured
fixed limits are available through `with_discovered_limits(false)` or the hosted route setting
`discover_model_limits = false`. The product developer loop performs this resolution itself.
Output-token limits are advisory. Text and Peritus tool history are supported; native session
continuations, image attachments, explicit reasoning controls, and server cancellation are
not currently advertised. Local cancellation terminates the owned process group.

The adapter requires the tested OpenCode 1.19.0 contract because isolation depends on its
configuration semantics. Setup installs that official version. Recognized subscription and
model-access rejections abort the vendor retry loop immediately. Other vendor retries retain
the official CLI behavior inside the bounded invocation. See the real-interface campaign report
for bounded behavioral evidence.
