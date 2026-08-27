# C5 model providers

C5 is Peritus's provider-neutral model request, streaming, and transport boundary. It consists of
six internal libraries rather than an agent loop, credential broker, or policy engine:

- `peritus-model-protocol` owns the versioned bounded request and normalized event contract;
- `peritus-provider-core` owns checked HTTP and process values, credential resolution, transport,
  framing, retry planning, cancellation, and effect ownership;
- `peritus-provider-openai` implements the official OpenAI Responses dialect and a separately
  profiled Codex-account route through the official `codex` executable;
- `peritus-provider-anthropic` implements the official Anthropic Messages dialect and a separately
  profiled Claude-account route through the official `claude` executable;
- `peritus-provider-google` implements Google's stable-v1 Interactions and Generate Content
  dialects; and
- `peritus-provider-compatible` implements only explicitly configured Responses or Chat
  Completions dialects.

These libraries are consumed later by C6 context construction and D0 turn orchestration. C5 does
not authorize tool calls, grant capabilities, mint budgets, persist conversations, or decide that
a model result should be accepted.

## Request boundary

Every `ModelRequest` is bound to protocol major version one, an immutable provider-profile identity
and revision, a checked model name, and a request identity used for observation. Its semantic form
contains ordered messages, bounded multimodal content, tools and schemas, structured-output policy,
reasoning and sampling controls, output limits, cache policy, provider state/continuation identity,
and explicitly negotiated provider extensions.

The canonical request encoding is provider-independent. It binds every semantic field and content
digest in stable order, excludes credentials and observation-only request identity, and produces the
input for deterministic idempotency identity. Provider JSON ordering and additive wire fields do not
change that canonical identity. Changing a capability profile revision does.

All strings, collections, JSON schemas, inline media, provider extensions, messages, tools, and
request bytes have production ceilings. Referenced media carries identity and provenance but grants
no filesystem, URL, or artifact-read authority. A later composition layer must resolve such content
through its own authorized boundary.

## Capabilities and profiles

A provider profile is a complete three-valued capability table: supported, unsupported, or unknown.
Unknown never means supported. Independent entries cover streaming, tools and parallel calls, strict
structured output, caching, each media family, reasoning controls and summaries, exact resumption,
confirmed server cancellation, detailed usage and rate limits, provider-stored state, sampling, and
explicit provider extensions.

Negotiation is a checked intersection between the request and one exact profile revision. It may
reduce requested numeric limits and decline optional behavior; it cannot invent provider support.
Unsupported required behavior fails before credential resolution, encoding, or transport. Profiles
also bind wire dialect, context and output ceilings, provider/model facts, state semantics,
idempotency guarantees, cancellation/resumption behavior, and whether the declared output ceiling
is provider-enforced or advisory. Executable-backed profiles are advisory because their official
CLIs do not reliably expose a provider-enforced output-token ceiling.

Compatible endpoints use a stricter declarative profile. The operator must select Responses or Chat
Completions and declare paths, authentication placement, framing, request fields, event mappings,
terminal shapes, limits, retry guarantees, state behavior, and redaction paths. The baseline profile
assumes the minimum safe feature set, never OpenAI parity based on a similar URL or model name.

## Normalized streaming

Provider adapters translate private wire events into an ordered `EventEnvelope` grammar:

```text
ResponseStarted
  ItemStarted
    TextDelta | ReasoningSummaryDelta | ReasoningReplayDelta | RefusalDelta
    ToolCallStarted -> ToolArgumentDelta
  ItemCompleted
  Usage | RateLimit | Cache | ProviderEvent | Heartbeat
  Finish
ResponseCompleted | ResponseFailed | ResponseCancelled
```

Every envelope has a monotonic adapter-local sequence plus optional provider sequence and event
identity, and a digest of the exact provider event bytes. Exact repeated identity/digest pairs are
ignored. Reused identities with different bytes, contradictory ordering, duplicate lifecycle
events, or content after completion are failures.

The reducer requires item-before-delta, call-before-arguments, one response start, valid item
completion, and exactly one terminal outcome. UTF-8 text and tool-argument JSON can cross transport
and event boundaries, but completed content is exposed only after bounded assembly and validation.
An incomplete tool call is never executable. Refusal, malformed data, missing terminals,
interruption, timeout, or cancellation cannot be converted into partial success.

Usage observations retain optional input, cache-read, cache-creation, output, reasoning-output,
server-tool, total, and provider-estimated cost counters. Missing data remains unknown. Cumulative
and final observations must be monotonic; step-local counters are not silently added to cumulative
values. Rate-limit and cache observations are evidence only and cannot increase B1 authority or
change an authoritative budget.

## Transport, retry, and cancellation

`peritus-provider-core` exposes Peritus-owned HTTP, streaming, and bounded process interfaces.
Provider adapters do not expose Reqwest, Tokio, process handles, an official SDK, wire JSON structs,
raw response bodies, or credential types through their public APIs. The HTTP implementation uses
Rustls, disables redirects, and applies checked header, body, chunk, frame, and aggregate byte
limits. The process implementation accepts checked executable, argument, stdin, directory,
environment-removal, output, timeout, and cancellation values and always reaps its owned child.

Credentials are resolved from an opaque reference immediately before request construction. Secret
values are zeroized where copied and omitted from debug output, errors, canonical bytes, fixtures,
URLs, normalized events, and fake-server observations. Endpoints reject user information,
fragments, traversal, non-HTTP(S) schemes, and secret-bearing query parameters.

Retry planning distinguishes a failure before send, connect failure, temporary rate limit,
transient server error, ambiguous submission, accepted response without events, partial stream,
invalid request, authentication, refusal, malformed content, cancellation, and terminal completion.
Attempts, elapsed time, delay, retry-after, jitter, and cumulative bytes are bounded.

A fresh retry after ambiguous acceptance is legal only when the selected provider contract
documents create-request deduplication. A partial stream can continue only through a documented
exact cursor for the same response. Otherwise the ambiguity remains visible to the caller; C5 does
not silently spend twice. Cancellation is idempotent and interrupts connection, body read, framing,
and backoff work. Dropping an owned stream signals cancellation and does not detach background work.

Account-backed routes use the providers' official executables as credential-owning, stateless
routers. Peritus does not read, store, refresh, or reproduce their account tokens. It supplies one
isolated request, disables native executable tools and ambient integration surfaces, accepts only
bounded structured output, and converts tool-shaped output into inert proposals for later D0
authorization. A crash or uncertain completion is never blindly replayed.

Neither executable contract documents a portable typed rate-limit/retry-after grammar. Their
profiles therefore leave detailed rate-limit reporting unsupported and retain generic reported CLI
errors without parsing diagnostic prose. A2's mandatory rate-limit/transient cases qualify the real
provider-core retry planner and two real process invocations from explicit test-fixture causes; that
evidence does not claim the production CLI can classify an undocumented error.

## First-party contracts

The provider contracts were reviewed on 2026-08-24 against official documentation. Golden request,
stream, error, corrupt, and unknown-field fixtures live in their owning adapter crates; SDK types
are not their source of truth.

### OpenAI Responses

The OpenAI adapter implements `POST /v1/responses` with bearer authentication and optional validated
organization/project headers. It projects messages and heterogeneous input/output items, text and
multimodal content, function tools, strict `text.format` schemas, reasoning controls and opaque
replay state, sampling, cache controls, usage, request/response identities, and documented
background continuation.

Responses streaming is sequence-numbered and item/content indexed. Function arguments remain text
until their documented terminal event. Ordinary stream closure is only local cancellation.
Confirmed cancellation and `starting_after` cursor resumption apply only to eligible background
responses created with streaming. The public create contract does not promise idempotent request
deduplication, so the adapter never blindly replays a maybe-sent foreground request.

Reviewed sources: [create response](https://developers.openai.com/api/reference/resources/responses/methods/create),
[streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events),
[background mode](https://developers.openai.com/api/docs/guides/background), and
[structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs).

### OpenAI Codex account runtime

The separate `OpenAiCodexRuntime` dialect invokes an already-authenticated official `codex`
executable for one ephemeral turn. It uses an isolated working/configuration boundary, read-only
execution policy, disabled ambient instruction and native-tool surfaces, JSONL events, and a private
output schema. Credential and endpoint-routing environment overrides are removed. The decoder
rejects native tool activity, malformed or oversized JSONL, and output without a proven completed
turn, then normalizes validated text, inert tool-call proposals, usage, and terminal state.

Codex itself owns ChatGPT login, credential persistence, and refresh; Peritus never reads its token
store. The profile is deliberately narrower than direct Responses: stateless replay, no response
resume or remote cancel, local best-effort child cancellation, and an advisory output limit.

Reviewed sources: [Codex SDK](https://developers.openai.com/codex/sdk/) and
[Codex App Server authentication](https://developers.openai.com/codex/app-server/). The delivered
route uses the official executable directly rather than implementing OAuth or a persistent App
Server connection.

### Anthropic Messages

The Anthropic adapter implements `POST /v1/messages` with `x-api-key`,
`anthropic-version: 2023-06-01`, and validated optional beta headers. System instructions remain
top-level. User/assistant content projects text, images, documents, client tools/results, strict
schemas, prompt caching, thinking controls, signatures, and redacted replay state.

Its stream reducer enforces `message_start`, indexed content-block start/delta/stop,
`message_delta`, and `message_stop`; `ping` may occur anywhere and an SSE error may follow HTTP 200.
Partial `input_json_delta` is parsed only after block completion, and usage deltas are cumulative.
`pause_turn` requires semantic continuation rather than ordinary success. Messages documents no
retrieve, server-cancel, cursor-resume, or create-idempotency contract, so interruption remains
unconfirmed and maybe-sent work is not blindly repeated.

Reviewed sources: [create message](https://platform.claude.com/docs/en/api/messages/create),
[streaming messages](https://platform.claude.com/docs/en/build-with-claude/streaming),
[structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs), and
[extended thinking](https://platform.claude.com/docs/en/about-claude/models/extended-thinking-models).

### Anthropic Claude account runtime

The separate `AnthropicClaudeRuntime` dialect invokes an already-authenticated official
`claude -p` executable for one stateless turn. Safe mode is enabled; the native tool set, slash
commands, browser integration, MCP servers, and session persistence are disabled; and a fixed
system prompt plus private JSON schema constrain the result. Credential and endpoint-routing
environment overrides are removed. The decoder accepts only bounded schema-valid text, inert
tool-call proposals, usage, and terminal state.

Claude itself owns account login, credential persistence, and refresh; Peritus never reads its token
store or offers Anthropic login. The profile does not claim direct Messages features it cannot
prove, including remote cancellation/resume, cache semantics, multimodal input, or a hard output
ceiling.

Reviewed sources: [Claude Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview)
and [Claude plan usage update](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan).

### Google Gemini

The Google adapter has separate stable-v1 profiles for Interactions and Generate Content. It forces
the `/v1` surface instead of accepting an SDK's beta default. The mappings cover contents or steps,
system instruction, functions and results, structured response schemas, image/audio/document input,
thinking controls and opaque signatures, cached content, usage metadata, safety and finish reasons,
state retention, and documented continuation behavior.

The adapter authenticates with `x-goog-api-key`, never a query secret. Interactions defaults to
`store=false`; stored continuation is selected explicitly. Google does not document a universal
create-idempotency key, foreground cancellation acknowledgement, or portable rate-limit-header
grammar, so the normalized profile does not claim them.

Reviewed sources: [Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview),
[stable-v1 reference](https://ai.google.dev/api/interactions-api-v1),
[API versions](https://ai.google.dev/gemini-api/docs/api-versions), and
[thinking](https://ai.google.dev/gemini-api/docs/thinking).

### Explicit compatible endpoints

The compatible adapter implements two separate projections. A Responses profile does not imply
Chat Completions behavior, and a Chat Completions profile does not imply Responses item, reasoning,
state, or resumption behavior. A configured endpoint must map every requested field and every
terminal event needed to prove success. Missing mappings, incompatible authentication,
unrecognized successful shapes, and unsupported multimodal/tool/schema/reasoning behavior fail
closed.

The reference wire contracts are OpenAI's [Responses create](https://developers.openai.com/api/reference/resources/responses/methods/create)
and [Chat Completions create](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create),
but an endpoint gains capabilities only from its own validated profile and conformance evidence.

## Conformance and verification

The reusable A2 `provider_suite` is nonempty and creates a fresh subject for each case. Its fourteen
cases directly observe capability honesty, exact ordering/deduplication, fragmented tool calls,
malformed and incomplete streams, interruption, cancellation, authentication failure, rate-limit
and retry-after behavior, transient retry, ambiguous submission, usage accounting, redaction, and
adapter isolation. Each production adapter supplies a development-only bridge and runs the entire
suite against a fresh deterministic loopback fake HTTP server.

Deduplication evidence is dialect-aware rather than fabricated. Streaming dialects with provider
event identity must inject and suppress a real exact duplicate. A final-result-only executable
dialect still proves deterministic normalized ordering, but explicitly marks provider-event
deduplication inapplicable and reports zero duplicates.

The Codex-account and Claude-account subjects run the same fourteen cases against hermetic fake
executables. These fakes exercise exact argument/environment isolation, bounded stdin/stdout/stderr,
schema decoding, nonzero exit, cancellation, and child reaping without requiring a provider
installation, live account, or network. Separate portable provider-core process tests exercise the
same production transport's stdout/stderr overflow and wall-clock timeout paths. The direct API and
account-backed subjects remain separate; passing one does not qualify the other.

The fake server validates bounded exact or ordered-subset requests, scripts status/headers/chunked
bodies and deliberate closure, exposes synchronization points for cancellation, records only
allowlisted digests/counts, and joins all workers on drop. Required tests make no live network calls
and require no provider credentials.

Credentialed account-route qualification is retained as one live-account example in each owning
provider crate. The examples call `CodexRuntimeProvider` and `ClaudeRuntimeProvider` directly,
require authentication through the official executables, and fail unless event sequences are
contiguous, usage is observed, native tool activity is absent, exact fixed canary text is returned,
and the normalized terminal is successful. They consume account usage and therefore remain
explicit operator commands rather than part of credential-free Gate A.

The Verus functional core covers capability intersection, request bounds, reducer lifecycle and
terminal uniqueness, exact deduplication, fragment completion, monotonic usage, retry legality, and
the fact that provider observations cannot grant authority or budget. TLS, HTTP, async wakeups,
clocks, randomness, JSON/SSE decoding, and zeroization adapters remain narrow ordinary-Rust trust
boundaries. Proof cheats and broad external-body declarations are forbidden.

The focused C5 gate is:

```text
CARGO_BUILD_JOBS=2 cargo test --locked \
  -p peritus-model-protocol -p peritus-provider-core \
  -p peritus-provider-openai -p peritus-provider-anthropic \
  -p peritus-provider-google -p peritus-provider-compatible \
  -p peritus-conformance -p peritus-test-support --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo clippy --locked <same packages> \
  --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=2 RUSTDOCFLAGS='-D warnings' cargo doc --locked \
  --no-deps --document-private-items
CARGO_BUILD_JOBS=1 just verus-verify
CARGO_BUILD_JOBS=1 just verus-build
CARGO_BUILD_JOBS=1 just gate-a
```

`just gate-a` remains the merge authority. Hosted Ubuntu, macOS, Windows, supply-chain, workflow,
policy, and Verus checks must pass on the signed pull request, followed by fresh push-only checks on
the signed merge commit.

## Remaining boundaries

C5 is a library layer, not a complete coding agent. C6 will construct role-scoped context,
compaction plans, and memory selection. D0 will durably compose C5 model streams with C4 tool
authorization/execution, lifecycle recovery, and completion proposals. C7 later owns telemetry and
encrypted raw-provider evidence. G0 later owns daemon credential brokering and provider worker
supervision; C5 owns only the bounded lifetime of each account-runtime child it directly launches.
None of those later responsibilities are implemented or bypassed by C5.
