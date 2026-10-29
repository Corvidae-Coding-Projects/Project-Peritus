# peritus-provider-openai

## Folder guide

First-party OpenAI Responses adapter for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-openai`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [fixtures/](fixtures/AGENTS.md) | Retained wire/provider fixture inputs for `peritus-provider-openai`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code. |
| [src/](src/AGENTS.md) | OpenAI HTTP Responses adapter and official Codex account runtime transport, with separate request/stream/runtime paths. |

First-party adapters for the current OpenAI Responses API and the pre-authenticated OpenAI Codex
account runtime. Both expose only Peritus protocol/core types; HTTP, JSONL, process, and provider
wire shapes remain private implementation details.

## Construction and profile boundary

`OpenAiConfig::new` fixes production traffic to `https://api.openai.com`; callers supply only a
`CredentialReference`, optional validated `org-`/`proj_` routing identities, and an optional bounded
`RetryPolicy`. `OpenAiClient::new(config, profile, credential_source)` owns its hardened Reqwest
transport. Credentials are resolved from `CredentialSource` immediately before each validated
submission; raw credentials and transport handles are not public constructor arguments.

The immutable `ProviderProfile` must name provider `openai`, use `WireDialect::OpenAiResponses`,
and support streaming. Request encoding is exact-capability-gated, including tools, parallel calls,
strict structured output, sampling, cache, media, reasoning, storage, and continuation. Unsupported
or profile-inconsistent values fail before credentials or transport.

## Lifecycle and recovery

The Codex runtime projection explicitly continues the assistant after the latest retained message.
A fresh Codex subprocess is transport for a provider step, not a new Peritus host invocation;
replayed task-entry instructions do not require restarting completed setup on each request.

Foreground requests stream locally and ordinary disconnect/caller cancellation is unconfirmed
local abort. Stored background requests may register their response identity for exact
`starting_after` continuation only after the adapter observes `response.created`; arbitrary or
foreign identities cannot be resumed. A `BackgroundResumable` profile may additionally advertise
confirmed cancellation, in which case `cancel_response` uses the authenticated
`POST /v1/responses/{id}/cancel` contract for identities observed by that adapter instance.

Create requests claim no idempotency guarantee. Pre-submission connect/TLS failures and explicit
temporary 429/eligible 5xx rejections follow the configured bounded retry policy. Quota,
authentication, permission, invalid-request, and malformed responses stop. Other transport failure
after submission is normalized as ambiguous acceptance and is never recreated blindly. Stream
failure after events are visible is terminal and never retried.

The SSE decoder validates event names, sequence numbers, identities, indices, lifecycle state,
heterogeneous output items, fragment completion, final usage, and explicit terminals. Exact
duplicates are ignored; reordered/conflicting sequences, unknown correctness-critical events,
corrupt JSON, premature EOF, and contradictory completion fail closed. Ancillary events are
preserved only as bounded provider extensions. Debug output, errors, and conformance observations
exclude credentials, prompts, model output, tool arguments, raw error bodies, and raw SSE.

## Codex account runtime

`CodexExecutable::discover` resolves and canonically pins the official `codex` executable once;
`CodexExecutable::pin` supports an explicitly managed path. Construct
`CodexRuntimeConfig::new(executable, profile, process_limits)` and then
`CodexRuntimeProvider::new(config)`. The provider owns the shared production process transport. It
delegates the first authentication check to `codex login status` and retains successful login availability for the provider lifetime and directs
unauthenticated users to run `codex login`; Peritus never reads credential storage and never starts,
refreshes, or exports an account login.

The exact runtime profile names provider `openai`, uses `WireDialect::OpenAiCodexRuntime`, declares
an advisory output limit, stateless local replay, unsupported resume, and best-effort local
cancellation. It supports bounded inline images, inert host tool proposals, bounded parallel host
tool proposals, portable reasoning-effort selection, and usage detail. High effort is the product
default; a concrete minimal/low, medium, or high request is projected to the official executable.
Automatic provider-managed prompt caching uses the official app-server's framed stdio interface.
A finite setup creates a routing-only local anchor containing the native context and a constant
host marker, with no inference or Peritus task data. The anchor uses legacy history so a user
setting selecting paginated history cannot make it unreadable. Each request loads that parent
and uses its official ephemeral prediction fork, then injects the complete current Peritus
request as user data. Stable routing-frame identity and metadata preserve cache affinity; task
instructions, source bytes, tool results and images still come from the current host snapshot.
No previous Peritus task or model output is inherited from the anchor. The anchor is small local
Codex metadata; child task history remains ephemeral. Cache hits and expiry are vendor-managed,
and actual cached-input counters remain distinct from local unchanged-prefix estimates.

Unsupported setup falls back to ordinary isolated turns with a diagnostic. A rejected
resume/fork/injection before model submission disables that unusable anchor and falls back
without replaying accepted inference. Once turn submission starts, failures remain explicit;
there is no automatic resubmission. Error categories and any reported usage survive normalization.
Explicit cache keys and TTL breakpoints, audio, documents, remote media, remote
persistence/background execution, continuation, reasoning replay or summaries, sampling controls,
and caller-defined strict structured output are rejected before authentication or process
submission.

The provider reuses one private working directory. Native per-inference time reminders,
independent Codex memories/compaction, skill discovery and execution features are disabled by
explicit overrides because Peritus owns those responsibilities. Cache-disabled or fallback turns
use ordinary JSONL `exec --ephemeral`, ignore user config/rules and Git state, and use separate
private schemas, images and final-response artifacts. Both paths remove OpenAI/Codex credential,
endpoint, organization and project overrides from the child environment. The output contract
uses a bounded tool-name enum plus an `arguments_json` string. Full host schemas are guidance;
returned names and argument objects are parsed and validated before becoming inert Peritus tool
proposals. Native execution items, malformed envelopes, missing completion and unknown
correctness-critical shapes fail closed. The first structurally valid nonempty host-tool batch
owns that step; later narration cannot replace it before Peritus supplies its results. The cached
path uses the final structured agent message; ordinary exec uses the private final-message file.
Required tool choices and argument object encoding are validated before any proposal is emitted.
Specific redaction-safe
diagnostics and any valid observed usage survive rejected final output. Native execution is a
non-retryable safety terminal, not a recoverable format error or provider-permission fallback.

PNG, JPEG, WebP, and GIF bytes remain outside the serialized text prompt. The text projection
records only attachment index, media type and SHA-256. The cached structured path supplies actual
inline image content to the official client. Ordinary exec writes images into the per-turn
private directory and supplies `--image`; directory lifetime covers the child process.

Subprocess stdin/stdout/stderr, timeout, cancellation, kill, and reap are bounded by
`ProcessLimits` and the shared process transport. A normal completed turn is buffered and validated
before normalized events are exposed. Cancellation is local best effort. A crash after submission
is partial or ambiguous according to observed JSONL, and the adapter never automatically replays a
turn. Codex CLI error text is never retained. The runtime recognizes only stable structured codes
or current official message families for authentication, safety policy, rate limits, quota, and
context limits, projects those to redaction-safe normalized categories, and treats every other
reported error generically. A non-retryable terminal keeps that category and its stable diagnostic
identity through the product result rather than becoming an unexplained empty response.

### Live account qualification

After `codex login status` confirms an authenticated official CLI, run the retained credentialed
probe from the repository root:

```text
CARGO_BUILD_JOBS=1 cargo run --locked --package peritus-release-qualification --example codex-live-account --all-features
```

An optional final argument overrides the default `gpt-5.6-sol` model. The probe goes through
`CodexRuntimeProvider`, not a direct CLI shortcut. It requires contiguous normalized events,
usage, an exact fixed canary, no tool activity, and a completed terminal. The command consumes
account usage and is intentionally not executed by credential-free Gate A.

## Compatibility corpus

`fixtures/v1` is the immutable official-contract corpus for this adapter revision. `MANIFEST`
records provenance and intent; `SHA256SUMS` covers every payload fixture. Tests verify the complete
directory inventory, request goldens, fragmented success/tool/reasoning streams, exact duplicate
handling, malformed/incomplete/unknown events, HTTP authentication/rate/quota/transient cases, and
the fourteen-case A2 Responses bridge against fresh deterministic fake servers. The same versioned
corpus contains current Codex JSONL success, tool, duplicate, malformed, incomplete, authentication,
and forbidden-native-tool cases plus byte-exact prompt/schema goldens. A feature-gated portable Rust
fake executable drives a separate fourteen-case production-process bridge; it is not built without
`test-runtime-fake`.

Direct dependencies are limited to Peritus protocol/core/foundation crates plus Serde, JSON,
base64 projection, and temporary-directory isolation. Conformance, fake HTTP servers, and the Codex
fake executable are development-only surfaces.

Focused verification from the repository root:

```text
cargo fmt -p peritus-provider-openai -- --check
CARGO_BUILD_JOBS=2 cargo test -p peritus-provider-openai --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-provider-openai --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" CARGO_BUILD_JOBS=2 cargo doc -p peritus-provider-openai --all-features --no-deps
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-provider-openai --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
CARGO_BUILD_JOBS=1 cargo verus build --package peritus-provider-openai --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.

## Optional direct ChatGPT OAuth preview

`ChatGptLogin` registers Peritus independently through OpenAI's documented open-source OAuth
flow. `ChatGptAccount` exposes non-secret registration metadata and live model discovery.
`ChatGptProvider` composes the ordinary Responses transport with rotating account credentials
and a separate supported request profile. It starts no Codex subprocess.

The launcher exposes this as **ChatGPT direct OAuth (preview)** in `peritus providers`.
It never migrates a saved Codex selection. On Unix, independent session files are atomic and
owner-only beneath the product state root's `chatgpt-oauth` directory; on Windows files inherit a restricted
owner/SYSTEM/Administrators ACL from the protected directory. Access and refresh tokens never enter
product state, generated daemon config, model input, or diagnostics. Refresh and sign-out hold a
cross-process account lock. Sign-out clears tokens but retains the issued registration identity.

The wire profile requires streamed, stateless replay (`store=false`). It translates system text
into `instructions`, groups Peritus functions in a namespace, and omits unsupported sampling,
background, continuation, and output-ceiling fields. The output allowance is advisory, never a
task-wide model token budget. Codex and API-key profiles keep their existing request contracts.

Official contract: https://developers.openai.com/siwc/token-sharing-open-source
