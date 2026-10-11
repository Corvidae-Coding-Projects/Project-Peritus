# Feature: C5 Production Model Providers

## Summary

C5 adds the complete model-provider boundary consumed later by C6 context selection, D0 agent
execution, E2 diagnosis, E3 evaluation, and G0 daemon composition. It introduces six cohesive
crates under `crates/model/`:

- `peritus-model-protocol` owns provider-neutral, versioned, bounded request and normalized stream
  semantics;
- `peritus-provider-core` owns transport, cancellation, retry, idempotency, endpoint, credential,
  redaction, bounded HTTP and subprocess mechanics without provider-specific domain policy;
- `peritus-provider-openai`, `peritus-provider-anthropic`, and `peritus-provider-google` own
  first-party wire projection and normalization for their documented APIs. The OpenAI and
  Anthropic crates also own separately profiled account-backed routes through the providers'
  official, already-authenticated executables; and
- `peritus-provider-compatible` owns explicitly profiled OpenAI-compatible Responses and Chat
  Completions endpoints without inferring unsupported behavior.

This slice is a production boundary, not an MVP. C5 is complete only when every crate, adapter,
compatibility fixture, A2 conformance case, formal obligation, documentation surface, local gate,
hosted pull-request gate, and fresh-main gate defined here passes together.

## User-visible behavior

C5 is a library layer and adds no CLI or daemon command. Later consumers can:

1. construct one validated `ModelRequest` without importing a provider SDK or wire type;
2. negotiate the exact capabilities and limits of a revision-bound provider profile before
   submission;
3. start a cancellable owned provider request with a deterministic idempotency key;
4. consume one ordered normalized stream containing content, reasoning summaries, tool calls,
   usage, cache, rate-limit, response identity, finish, and failure observations;
5. distinguish refusal, invalid input, authentication, rate limiting, transient service failure,
   transport failure, ambiguous acceptance, malformed payload, incomplete stream, cancellation,
   and terminal success;
6. reject incomplete tool calls and malformed or contradictory terminal data before later code can
   treat them as executable; and
7. select a first-party or compatible adapter from an explicit capability profile without changing
   provider-neutral orchestration logic; and
8. use an existing Codex or Claude subscription through the provider's own credential-owning
   executable without Peritus reading, storing, refreshing, or reproducing account credentials.

Provider requests and observations are bounded, redacted, deterministic where C5 owns their form,
and safe to persist through later B3/C0 projection code. Provider reports are observations only:
they cannot authorize tools, amend policy, mint budget, refund authoritative consumption, or claim
run acceptance.

## Requirements

### Protocol and compatibility

- **C5-R-001 — Versioned protocol.** Every request, capability profile, normalized event, terminal
  result, and compatibility fixture names protocol major version one. Unknown major versions are
  rejected; additive unknown provider fields remain wire-local and never become implicit
  capabilities.
- **C5-R-002 — Validated identities.** Provider, model, request, response, item, tool-call, event,
  and deterministic idempotency identities are nonempty, bounded, canonical values. Raw
  credentials are not identities and never enter canonical request bytes.
- **C5-R-003 — Complete content model.** Messages cover system/developer/user/assistant/tool roles;
  bounded text; referenced or bounded-inline image, audio, and document input; assistant output;
  tool calls/results; refusal; and provider-visible metadata needed for resumption. Unsupported
  blocks fail capability negotiation before transport.
- **C5-R-004 — Structured output and tools.** Requests carry bounded JSON Schema, tool choice,
  parallel-call policy, and strict structured-output policy without depending on C4 crates. D0 will
  explicitly map C4 tool descriptors/results into this provider-facing form.
- **C5-R-005 — Deterministic canonical form.** Canonical request bytes bind protocol version,
  profile revision, model, messages, content digests, tools, output policy, reasoning controls,
  sampling controls, limits, cache directives, and continuation identity in a stable order.
- **C5-R-006 — Compatibility fixtures.** `peritus-model-protocol/fixtures/v1/` contains minimal,
  realistic, boundary, corrupt, and unknown-field fixtures. Decode/encode compatibility is tested
  and fixture drift is reviewed.

### Capability and profile semantics

- **C5-R-007 — Explicit capabilities.** A `ProviderProfile` declares streaming, tool calls,
  parallel tool calls, strict structured output, prompt caching, image/audio/document input,
  reasoning controls and summaries, resumable response identity, cancellation, usage detail,
  rate-limit detail, supported wire dialect, context/output limits, and provider-owned model facts.
  It also declares whether the output ceiling is provider-enforced or advisory; orchestration must
  never treat an advisory executable limit as a hard remote guarantee.
- **C5-R-008 — Exact negotiation.** Negotiation is a pure checked intersection. A requested feature
  missing from the profile is rejected before encoding. Negotiation can reduce limits or disable
  optional behavior but cannot synthesize support.
- **C5-R-009 — Revision binding.** Profile identity and revision participate in request identity,
  evidence, conformance observations, and idempotency. A profile change never silently mutates an
  in-flight or replayed request.
- **C5-R-010 — Compatible endpoints are declarative.** Compatible profiles explicitly name wire
  dialect, paths, auth placement, headers, query parameters, stream framing, supported request
  fields, event mappings, limits, retry guarantees, and response-ID semantics. Defaults are the
  minimum safe feature set, not presumed OpenAI parity.

### Streaming and reduction

- **C5-R-011 — Normalized grammar.** Normalized events cover response start, item start, text and
  reasoning deltas, tool-call start and argument deltas, item completion, usage/cache/rate-limit
  observations, response identity, finish, refusal, and structured failure.
- **C5-R-012 — Ordered bounded reduction.** The reducer enforces one response start, monotonic local
  sequence, item-before-delta, call-before-arguments, item completion before successful terminal,
  event/item/output ceilings, and exactly one terminal outcome.
- **C5-R-013 — Safe deduplication.** An exact repeated provider event identity with the same digest
  is ignored. Reuse with different bytes, contradictory sequence, duplicate start/end, or delta
  after completion is malformed input and terminates non-successfully.
- **C5-R-014 — Fragment assembly.** UTF-8 text and JSON tool arguments may cross transport chunks
  and provider events. Assembly is incremental and bounded; incomplete UTF-8, invalid terminal
  JSON, duplicate tool-call identities, or unterminated calls cannot become executable output.
- **C5-R-015 — No partial success.** Transport EOF, cancellation, timeout, provider error, refusal,
  malformed payload, missing required terminal event, or stream interruption yields a specific
  non-success result even when earlier content was valid.
- **C5-R-016 — Backpressure and ownership.** Byte and event channels are bounded. Dropping an owned
  stream cancels its request and observes worker completion; no background task is detached.

### Usage, retry, cancellation, and redaction

- **C5-R-017 — Complete usage observation.** Usage represents input, cached input, cache creation,
  output, reasoning output, and provider-cost microunits when known. Values are checked,
  nonnegative, monotonic high-water observations. Missing detail stays unknown rather than zero.
- **C5-R-018 — Rate-limit/cache observations.** Provider windows, remaining quantities, reset
  instants/durations, cache keys/status, and response metadata are bounded observations with source
  provenance. They do not alter B1 budgets or policy.
- **C5-R-019 — Deterministic idempotency.** C5 derives a stable key from the canonical request and
  profile revision. Adapters send it only through provider-supported mechanisms and record whether
  the mechanism protects ambiguous acceptance.
- **C5-R-020 — Retry legality.** Retry decisions distinguish not-sent failure, connect failure,
  ambiguous submission, accepted response without events, partial stream, rate limit, transient
  server error, invalid request, authentication, refusal, malformed content, cancellation, and
  terminal completion. Ambiguous or partially observed requests are never blindly resubmitted.
- **C5-R-021 — Retry bounds.** Attempts, exponential delay, jitter input, retry-after, elapsed time,
  and cumulative bytes are bounded. The deterministic planner accepts clock/random observations;
  the transport shell only executes an already-checked plan.
- **C5-R-022 — Cancellation.** Cancellation is idempotent, interrupts pending connection/read/backoff
  work, produces one terminal cancellation observation, and joins owned work.
- **C5-R-023 — Credential isolation.** Credentials enter only through a narrow redacted provider-core
  credential source at request time. They are zeroized where copied, omitted from `Debug`, errors,
  canonical bytes, fixtures, traces, URLs, and normalized events. Account-backed routes are
  stricter: the official executable exclusively owns login, token persistence, and refresh;
  Peritus neither requests nor reads the credential material.
- **C5-R-024 — Redacted diagnostics.** Request/response diagnostics allowlist names, status,
  provider request ID, content type, byte counts, timing, and stable categories. Bodies and headers
  are bounded and redacted before exposure; secret canaries never escape adapter tests.

### Provider adapters

- **C5-R-025 — OpenAI.** The OpenAI adapter implements the current official Responses request and
  streaming grammar, tool calls, strict structured output, multimodal input, reasoning controls and
  summaries, usage/cache data, response IDs, documented continuation/resumption, rate limits,
  error mapping, cancellation, and documented idempotency behavior.
- **C5-R-025A — OpenAI Codex account route.** The OpenAI crate provides a distinct
  `OpenAiCodexRuntime` profile and provider that invokes an already-authenticated official `codex`
  executable as a stateless structured inference router. It isolates configuration and working
  state, disables native tools and ambient project instructions, strips credential and routing
  environment overrides, rejects unexpected executable item types, and normalizes only validated
  text, inert tool-call proposals, usage, and terminal state. It does not implement ChatGPT login,
  inspect auth storage, or claim API-key Responses capabilities that the executable route has not
  proven.
- **C5-R-026 — Anthropic.** The Anthropic adapter implements the current official Messages request
  and SSE grammar, system/content blocks, tool use/results, structured output where documented,
  images/documents, thinking controls/blocks, prompt caching, usage, request IDs, rate-limit/error
  mapping, required API version/beta headers, cancellation, and documented retry semantics.
- **C5-R-026A — Anthropic Claude account route.** The Anthropic crate provides a distinct
  `AnthropicClaudeRuntime` profile and provider that invokes an already-authenticated official
  `claude -p` executable as a stateless structured inference router. It uses safe mode, disables
  native tools, slash commands, browser integration, MCP servers, and session persistence, strips
  credential and routing environment overrides, and normalizes only schema-validated text, inert
  tool-call proposals, usage, and terminal state. It does not implement Claude login, inspect auth
  storage, or claim direct Messages API capabilities that the executable route has not proven.
- **C5-R-027 — Google.** The Google adapter implements the stable-v1 Gemini Interactions API and
  the still-supported stable-v1 `generateContent`/streaming grammar. It covers contents/steps,
  system instruction, function declarations/calls/results, response schema, image/audio/document
  input, thinking controls and opaque replay signatures, cached content, usage metadata,
  finish/safety reasons, error mapping, cancellation, retention/state policy, and documented retry
  semantics. It never silently uses an SDK's `v1beta` default.
- **C5-R-028 — Compatible.** The compatible adapter supports only the explicitly selected Responses
  or Chat Completions projection and the fields/events declared by its validated profile. It rejects
  missing mappings, incompatible auth, unsupported multimodal/tool/structured-output/reasoning
  requests, and unrecognized successful terminal shapes.
- **C5-R-029 — No wire leakage.** Provider request/response structs are private to adapter crates.
  Every public adapter API accepts or returns only C5 protocol/core and foundation types.
- **C5-R-029A — Bounded executable ownership.** Provider-core exposes a provider-neutral process
  effect shell with explicit executable, arguments, stdin, working directory, environment
  removals, output ceilings, deadline, and cancellation. A launched child is always killed when
  required and reaped before ownership ends. Provider-specific flags, schemas, transcript
  projection, and output decoding remain private to the OpenAI or Anthropic crate.

### Formal and repository requirements

- **C5-R-030 — Verified functional core.** Verus covers capability intersection, bounds
  completeness, reducer state transitions, terminal uniqueness, event deduplication legality,
  fragment completion predicates, monotonic usage, retry legality, and the rule that provider
  observations cannot increase authority or authoritative budget.
- **C5-R-031 — Narrow trust boundary.** TLS, HTTP, async wakeups, clocks, randomness, JSON/SSE wire
  parsing, and zeroization adapters are H/T code behind checked values. Trusted/excluded code is
  inventoried; proof cheats and broad external-body declarations are forbidden.
- **C5-R-032 — Maintainable layout.** Root modules only document and re-export. Each responsibility
  has a named module; generic `utils`, `helpers`, `common`, or manager dumping grounds are forbidden.
- **C5-R-033 — Workspace registration.** All crates inherit workspace metadata/lints, use exact
  dependency pins, and are registered as C5 model-layer packages in `Cargo.toml` and
  `architecture.toml` with their H/T verification classes.
- **C5-R-034 — Complete documentation.** Every crate has a README, public API documentation,
  failure/compatibility notes, tests, and explicit dependency policy. `docs/c5-model-providers.md`,
  root README state, and CHANGELOG describe the delivered boundary and remaining C6/D0 work.

## Acceptance criteria

1. All six C5 crates exist under `crates/model/`, are registered, build in ordinary Rust, and obey
   architecture/model-layer policy.
2. A complete representative request containing multimodal input, tools, strict structured output,
   reasoning controls, cache policy, and continuation identity round-trips through protocol v1
   fixtures with stable canonical bytes and digest.
3. Capability tests vary every independent feature and limit; no unsupported feature reaches an
   adapter encoder or transport.
4. Reducer tests cover complete text, reasoning, structured output, parallel tool calls, fragmented
   arguments/UTF-8, exact duplicates, conflicting duplicates, reorder, over-limit data, refusal,
   malformed terminal, missing terminal, interruption, and cancellation.
5. Usage tests prove monotonic checked observations and demonstrate that corrections/refunds cannot
   increase B1 authority or produce negative consumption.
6. Retry-table tests cover every C5-R-020 phase/failure pair, bounded retry-after/backoff, ambiguous
   acceptance, partial streams, resumable response identity, cancellation during backoff, and
   exhaustion.
7. Credential/redaction tests inject unique canaries through each adapter and prove they are absent
   from errors, debug output, normalized events, recorded fake-server observations, and artifacts.
8. The A2 `provider_suite` is nonempty, creates a fresh subject for every case, and directly checks
   capability honesty, ordering/deduplication, fragments, malformed/incomplete streams, retries,
   cancellation, auth/rate/transient errors, ambiguous submission, usage, redaction, and adapter
   isolation.
9. OpenAI, Anthropic, Google, and compatible production subjects each pass the entire A2 provider
   suite against deterministic fake HTTP servers plus provider-specific wire fixture tests. The
   Codex-account and Claude-account production subjects independently pass the same suite against
   deterministic fake executable subjects; no required test uses a real account or network.
10. Provider-specific golden requests and streams are derived from current official documentation,
    cite the reviewed contract/version in `docs/c5-model-providers.md`, and contain corrupt and
    unknown-field cases.
11. Public-API inspection finds no provider SDK/wire type, raw credential, unbounded raw payload,
    Tokio/Reqwest/process handle, or C4 type in a C5 public signature.
12. Verus verifies the C5 functional-core obligations and builds the verified release roots with no
    new unreviewed trust or proof-cheat findings.
13. Focused tests, full workspace tests/build/docs, strict Clippy/rustdoc, architecture policy,
    ordinary API policy, license/source policy, `just verus-verify`, `just verus-build`, and
    `just gate-a` pass with the required memory limits.
14. The signed C5 commits merge only after hosted Ubuntu, macOS, Windows, supply-chain, workflow,
    policy, and Verus checks pass; fresh push-only workflows for the merge commit also pass.

## Current architecture

The repository currently implements A0–A2, B0–B3, and C0–C4. B3 provides versioned bounded domain
commands/events and `ProviderProfileId`; B1 provides monotonic token/cost budget authority; A2
provides exact scripted fake-provider calls but deliberately no production provider trait. Its
`peritus.provider` conformance suite is empty pending C5. C4 defines model-facing tool descriptors
and calls, but architecture policy forbids model-layer production dependencies on the tools layer.

`architecture.toml` already reserves `crates/model` as a layer that may depend only on foundation
and model crates, with testing dependencies permitted for A2. No C5 crate, persisted C5 schema, or
provider production caller exists, so the v1 contract can be introduced without migrating stored
data or existing public callers. Future D0 is the first production composition consumer.

The Codex CLI reference demonstrates useful transport separation, owned streams, explicit retry
state, provider configuration, SSE parsing, response identities, rate-limit extraction, and fake
stream tests. C5 does not copy its OpenAI-shaped domain surface: Peritus keeps provider wire types
private and defines a provider-neutral reducer suitable for four distinct adapters.

OpenClaudia demonstrates the intended account-backed boundary for both providers: the official
Codex and Claude executables retain their provider-supported authentication, while the embedding
application invokes a tightly constrained stateless turn and validates bounded structured output.
C5 adopts that executable-as-router boundary without importing OpenClaudia code or treating its
behavior as provider authority.

## Proposed design

### Ownership and dependency flow

```text
peritus-types / peritus-codec / peritus-protocol
                     │
                     ▼
          peritus-model-protocol
                     │
                     ▼
           peritus-provider-core
          ┌──────────┼───────────┬────────────────┐
          ▼          ▼           ▼                ▼
       openai    anthropic     google         compatible
       ├─ API     ├─ API
       └─ codex   └─ claude
          CLI        CLI

peritus-test-support + peritus-conformance --dev-depend--> every production subject
```

Adapters do not depend on one another. `peritus-provider-compatible` may reuse provider-core
framing and protocol reduction but does not depend on or wrap `peritus-provider-openai`, preventing
first-party implementation assumptions from becoming compatible defaults.

### `peritus-model-protocol`

The crate is divided into `version`, `bounds`, `identity`, `capability`, `content`, `message`,
`schema`, `tool`, `request`, `canonical`, `event`, `reducer`, `usage`, `rate_limit`, `finish`,
`failure`, `retry`, `redaction`, and `verified` modules. `lib.rs` documents invariants and exports
the stable contract only.

All strings, vectors, schemas, binary input, and streams have checked constructors and explicit
limits. Inline binary media is allowed only under a profile/request byte ceiling; artifact or URI
references carry media type, digest where available, and provenance but no ambient read authority.
The reducer owns assembled content and exposes immutable completed items only after their closing
event validates. In-progress tool arguments cannot be extracted as a `CompletedToolCall`.

The reducer's pure state is projected into a compact Verus model containing phase, last sequence,
open/closed item and call identities, cumulative byte/event counts, high-water usage, seen event
identity/digest pairs, and terminal state. Ordinary collections refine this model through checked
transition functions. Proof obligations establish bounds, legal ordering, exact deduplication,
terminal uniqueness, completed-tool-call construction, and monotonic observations.

### `peritus-provider-core`

The crate is divided into `adapter`, `endpoint`, `credential`, `http`, `process`, `transport`,
`stream`, `framing`, `retry`, `backoff`, `cancellation`, `redaction`, `diagnostic`, and
`reqwest_transport`.
It defines object-safe standard-library-future interfaces rather than exposing Tokio, Reqwest, or
provider SDK types:

- `ModelProvider` returns a checked profile and asynchronously starts an `OwnedModelStream`;
- `ModelStream::next` yields normalized events through a boxed `Future`;
- `HttpTransport` consumes a bounded private `HttpRequest` and returns owned response metadata plus
  a pull-based bounded `ByteStream`; and
- `CredentialSource` resolves an opaque reference immediately before request construction.

The process effect shell accepts only checked, bounded values and returns bounded stdout, stderr,
exit state, and timing observations. It exposes no Tokio process handle. Cancellation and timeout
kill and reap the owned child. The shell owns no provider flag, authentication, transcript, retry,
or output-decoding policy; those remain in the adapter using it.

The Reqwest/Rustls implementation is the default production transport. Endpoint validation rejects
userinfo, fragments, non-HTTP(S) schemes, unsafe header names, secret-bearing query parameters, and
path traversal. Redirects are disabled unless the adapter's fixed first-party policy explicitly
allows an exact same-origin redirect. Response bodies and headers are bounded before parsing.

Cancellation ownership is shared between the returned stream and its worker. Dropping the stream
signals cancellation and joins through an owned completion path. Retry planning is pure; the effect
shell supplies time, jitter, transport phase, provider status, retry-after, idempotency support, and
resumption identity, then executes only `RetryPlan` values accepted by the protocol.

### Provider adapters

Each adapter uses `config`, `profile`, `request`, `stream`, `error`, `client`, and `conformance`
modules. Wire structs are private and `serde(deny_unknown_fields)` is used only where the provider
contract is closed; evolving event envelopes preserve unknown fields privately while still
rejecting unknown event kinds needed for terminal correctness.

- OpenAI projects to Responses and normalizes its documented streaming event types. Response and
  item IDs are retained for continuation and exact deduplication. Ordinary stream disconnect is
  only a local abort; exact cursor continuation is restricted to a background response created
  with streaming. The adapter never claims request-creation idempotency because the public
  Responses contract documents no idempotency-key guarantee.
- The OpenAI account route invokes `codex exec` for one ephemeral turn with isolated configuration
  and working state, read-only execution policy, ambient instruction loading disabled, native tools
  disabled, JSONL event output, and a private output schema. It rejects native tool activity and
  requires a validated turn-complete event before reporting success. Codex owns login and token
  refresh; Peritus exposes the provider executable's own login-status command rather than auth
  material. Because the route cannot reliably impose a provider-side output-token ceiling, its
  profile marks that limit advisory.
- Anthropic projects to Messages, supplies the required API-version headers, and reduces content
  block start/delta/stop plus message terminal events. It preserves thinking signatures and
  redacted-thinking bytes as sensitive opaque replay state, keeps cumulative usage semantics, and
  treats disconnect as unconfirmed cancellation. Messages likewise documents no idempotent-create
  guarantee.
- The Anthropic account route invokes `claude -p` for one turn with safe mode, an empty native-tool
  set, strict empty MCP configuration, slash commands/browser integration/session persistence
  disabled, a fixed system prompt, and a private JSON output schema. Claude owns login and token
  refresh; Peritus exposes the provider executable's own account/status workflow rather than auth
  material. Its output ceiling is likewise advisory.
- Google offers two explicit first-party dialect profiles. The preferred new-development profile
  projects stable-v1 Gemini Interactions steps and SSE events; the stable-v1 Generate Content
  profile covers existing `generateContent`/`streamGenerateContent` integrations. Step/candidate,
  part, function-call, thought-signature, safety, finish, retention, and usage semantics are
  normalized explicitly. The adapter forces `/v1` and does not inherit SDK `v1beta` defaults.
- Compatible projects to either Responses or Chat Completions according to a validated profile.
  Every non-core field and event mapping is allowlisted; a nominally successful but unmapped
  terminal response is malformed, not success.

Unknown additive provider fields and ancillary event variants may be retained in a bounded,
sensitive provider-event observation. An unknown variant that is required to establish content,
tool-call, usage, or terminal correctness is a malformed stream. This separates forward-compatible
observation from false success.

### Reviewed provider contracts

The contract review date is 2026-08-24. Adapter fixtures and implementation notes cite these
official sources rather than an SDK's translated types.

- OpenAI Responses: [create response](https://developers.openai.com/api/reference/resources/responses/methods/create),
  [streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events),
  [background mode](https://developers.openai.com/api/docs/guides/background),
  [structured output](https://developers.openai.com/api/docs/guides/structured-outputs), and
  [rate limits](https://developers.openai.com/api/docs/guides/rate-limits).
- OpenAI account runtime: [Codex SDK](https://developers.openai.com/codex/sdk/) and
  [Codex App Server authentication](https://developers.openai.com/codex/app-server/). The delivered
  stateless profile uses the official Codex executable rather than implementing an independent
  OAuth client or persistent App Server transport.
- Anthropic Messages: [create message](https://platform.claude.com/docs/en/api/messages/create),
  [streaming grammar](https://platform.claude.com/docs/en/build-with-claude/streaming),
  [structured output](https://platform.claude.com/docs/en/build-with-claude/structured-outputs),
  [extended thinking](https://platform.claude.com/docs/en/about-claude/models/extended-thinking-models),
  and [rate limits](https://platform.claude.com/docs/en/api/rate-limits).
- Anthropic account runtime: [Claude Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview)
  and [Claude plan usage update](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan).
  The route runs the official CLI programmatically and does not offer or reproduce Claude login.
- Google Gemini: [Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview),
  [stable-v1 reference](https://ai.google.dev/api/interactions-api-v1),
  [API-version policy](https://ai.google.dev/gemini-api/docs/api-versions),
  [Interactions migration](https://ai.google.dev/gemini-api/docs/migrate-to-interactions),
  [thinking](https://ai.google.dev/gemini-api/docs/thinking), and
  [rate limits](https://ai.google.dev/gemini-api/docs/rate-limits).
- OpenAI-compatible reference dialect: [Chat Completions create](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
  and [streaming events](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events).

OpenAI output is an ordered heterogeneous item list, not an assumed first assistant message.
Function arguments remain incomplete strings until their terminal event. Background Responses may
be polled and explicitly cancelled, and only a background streaming response has a documented
`starting_after` cursor. Anthropic system instructions are top-level rather than a system-role
message, `input_json_delta` remains text until `content_block_stop`, `message_delta` usage is
cumulative, `ping` may occur anywhere, and an SSE error may arrive after HTTP success. Anthropic
`pause_turn` is semantic continuation, not terminal success. Both adapters preserve raw status,
finish, provider request identity, and evolving optional usage detail without interpreting missing
counters as zero.

Gemini Interactions is the GA stable-v1 surface Google recommends for new development, while
Generate Content remains an explicitly supported first-party dialect. Interactions steps,
`requires_action`, partial structured-output text, thought signatures, per-step versus cumulative
usage, storage retention, and `previous_interaction_id` are preserved rather than flattened into
message assumptions. C5 defaults to `store=false`; stored continuation/background execution must
be requested as a capability and retention policy. Google documents no create-idempotency key,
stable rate-limit header grammar, or foreground cancellation acknowledgement. Authentication uses
`x-goog-api-key`, never a secret query parameter.

The compatible crate treats Chat Completions and Responses as separate profile dialects. Each
profile binds its exact request fields, auth placement, schema subset, stream framing, delta versus
cumulative behavior, finish/error mapping, usage/rate paths, retry guarantees, cancellation,
retention, and redaction paths. A models-list response proves only model identity unless the
endpoint's own authoritative contract or a profile-bound conformance probe proves more.

### A2 provider conformance

`peritus-conformance::provider` replaces the empty catalog entry with a production-neutral
`ProviderConformanceSubject`, fixed scenario fixtures, direct observation types, and cases for all
acceptance criterion 8 behaviors. The generic A2 crate never imports a C5 crate. Each adapter owns a
small dev-only bridge that maps the C5 production subject into A2 observations.

`peritus-test-support` adds an owned deterministic fake HTTP server with scripted request matching,
chunk/SSE timing, disconnect/fault points, exact captured redacted observations, cancellation-safe
shutdown, and per-case isolated listeners. It remains protocol-neutral: provider-specific JSON and
event fixtures live in the owning adapter crates.

### Design alternatives

The preferred design uses a Peritus-owned normalized protocol plus private provider projections.
This costs explicit mapping code but gives stable compatibility, testable reducer semantics,
provider independence, and a narrow T boundary.

A credible alternative is to expose an OpenAI Responses-shaped interface and translate Anthropic
and Google into it. That reduces the first adapter's code but silently treats one provider's event
grammar, identifiers, tool semantics, and optional fields as universal. It also makes compatible
endpoints appear more capable than their profiles and raises long-term migration cost. Rejected.

Another alternative is to expose official SDK request/stream types behind a trait. It saves wire
maintenance but leaks dependency versions through public APIs, prevents common formal reduction,
makes replay fixtures provider-specific, and couples D0 to multiple SDK lifecycles. Rejected.

For account-backed access, a persistent Codex App Server or provider SDK session could reduce
per-turn startup cost. C5 instead defines a complete stateless router profile around the official
executables: it is easier to isolate, reap, fixture, and keep separate from D0 conversation state.
The profile is intentionally honest about its narrower capabilities. A future internal persistent
transport may implement the same public profile only if it preserves these semantics; it does not
require a new protocol dialect merely to change process lifetime.

## Data and compatibility

Protocol v1 fixtures are committed with content digests. Canonical request encoding is separate
from provider JSON encoding: provider field-order or additive-wire changes therefore do not change
Peritus idempotency. A profile revision changes canonical identity whenever capabilities, limits,
endpoint dialect, or retry/resumption guarantees change.

C5 writes no authoritative database rows. Later B3/C0 integration will persist normalized events
and terminal observations by protocol version. Raw provider bytes remain private transient data;
C7 may later store separately encrypted raw-vault artifacts, but C5 exposes only redacted bounded
diagnostics and digests.

Rollback before D0 consumption removes the six additive crates, A2 provider cases, fixtures, docs,
workspace registrations, and dependencies. After protocol v1 is consumed, rollback retains the v1
decoder/fixtures and disables adapters through composition rather than reinterpreting stored data.

## Failure handling

Every failure carries provider kind, stable C5 category/code, transport phase, retryability,
ambiguity, request/response identity when safe, retry-after when valid, and a bounded redacted
diagnostic. Sources are preserved internally without including secrets or raw bodies.

Unknown success event kinds, missing terminal events, invalid JSON fragments, contradictory usage,
over-limit bodies, stream idle timeout, connection loss, cancellation, and worker join failure are
all explicit non-success outcomes. Teardown failure cannot replace an earlier provider failure; it
is attached as additional infrastructure evidence. A panic in parsing/conformance is a failed case,
not a passing or skipped result.

Executable routes never automatically replay after submission: a crash, timeout, malformed output,
or uncertain completion is reported as ambiguous unless the provider's structured events prove the
turn was not accepted. Cancellation kills and reaps the child and produces one normalized terminal
observation. Nonzero exit, output overflow, stderr overflow, schema mismatch, unexpected native
tool activity, and missing turn completion are explicit failures.

The account-runtime profiles do not claim typed rate-limit detail unless the official executable
contract documents a stable machine-readable category and retry-after field. Required conformance
planner cases may use explicitly classified hermetic fixtures to exercise checked delays and real
process attempts, but those fixtures cannot widen the production profile or justify parsing
untrusted diagnostic prose.

## Security considerations

External provider content is untrusted data. Adapter parsers never interpret provider text as
configuration, policy, authorization, or harness instructions. URL and header validation occurs
before credential resolution. TLS certificate validation is enabled by default and cannot be
disabled by a compatible profile. Proxies, custom roots, and private endpoints are future
composition policy and are not enabled by ambient environment inside C5.

Credential values are short-lived zeroizing buffers, attached only to the documented auth header or
query location, and never cloned into diagnostics. Redirects cannot forward credentials across
origins. Decompression and stream parsing enforce compressed and expanded byte ceilings to resist
resource exhaustion. JSON recursion/member/string limits are checked after decoding and before
normalization.

Account-backed routes remove API-key, base-URL, proxy-routing, and provider credential overrides
from the child environment, use isolated per-turn directories, and never inspect the provider's
credential store. Prompts, model output, and executable diagnostics remain untrusted. The child has
no Peritus-granted tool authority: tool-shaped output is inert protocol data that D0 must separately
authorize in a later slice.

## Verification

Focused development checks use the mandated memory limits:

```text
CARGO_BUILD_JOBS=2 cargo test --locked \
  -p peritus-model-protocol -p peritus-provider-core \
  -p peritus-provider-openai -p peritus-provider-anthropic \
  -p peritus-provider-google -p peritus-provider-compatible \
  -p peritus-test-support -p peritus-conformance \
  --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo clippy --locked \
  -p peritus-model-protocol -p peritus-provider-core \
  -p peritus-provider-openai -p peritus-provider-anthropic \
  -p peritus-provider-google -p peritus-provider-compatible \
  -p peritus-test-support -p peritus-conformance \
  --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=2 cargo doc --locked --no-deps --document-private-items
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- ordinary-api-check
CARGO_BUILD_JOBS=1 just verus-verify
CARGO_BUILD_JOBS=1 just verus-build
CARGO_BUILD_JOBS=1 just gate-a
```

Provider fake-server tests run on Ubuntu, macOS, and Windows. They use loopback only, explicit
deadlines with hosted-runner scheduling allowance, and protocol state rather than fragile fixed poll
counts. Network-dependent live-provider tests are not release evidence and require no credentials
in CI.

## Rollout and rollback

1. Land the design, exact dependencies, six registered crate skeletons, complete protocol/core,
   formal roots, and nonempty A2 provider contract.
2. Freeze the protocol/core public surface after focused checks and a full workspace/Verus gate.
3. Implement direct OpenAI and Anthropic independently against the frozen contract; merge neither if it
   requires an unreviewed shared-protocol mutation.
4. Implement Google and compatible independently against the same frozen contract.
5. Add the separate Codex-account and Claude-account profiles, bounded process shell, fake
   executable subjects, and account-runtime fixtures without weakening either direct adapter.
6. Integrate compatibility fixtures, documentation, trust/obligation manifests, and every adapter's
   production conformance bridge.
7. Run the complete local gate, submit signed commits, require every hosted check, merge normally,
   and require fresh push-only main checks.

The slice can be reverted additively before D0. A provider can be disabled independently through
future composition without changing normalized protocol meaning. No intermediate stage is a
releasable product.

## Open questions

No product question blocks implementation. Provider wire versions, optional beta headers, retry
guarantees, and continuation mechanisms are resolved from current official documentation and
recorded in `docs/c5-model-providers.md`; any undocumented behavior remains unsupported rather than
guessed.

## Out of scope

- C6 context selection, compaction, memory, and role policy;
- D0 turn orchestration, tool authorization/execution, completion proposals, or persistence;
- D1 gate scheduling and acceptance;
- C7 telemetry export or encrypted raw-provider vault storage;
- G0 daemon credential brokering and long-lived provider worker supervision (the bounded per-turn
  official-executable routes delivered here remain C5 transport effects);
- model training, hosting, fine-tuning, or provider account management;
- live-provider credentials or nondeterministic internet calls in required tests; and
- silently compatible behavior not declared by a validated endpoint profile.
