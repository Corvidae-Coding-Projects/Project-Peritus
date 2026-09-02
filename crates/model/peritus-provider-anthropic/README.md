# peritus-provider-anthropic

First-party Anthropic Messages and account-backed Claude executable adapters for Peritus. The public
boundary consists of `AnthropicConfig`, `AnthropicBeta`, `AnthropicClient`, exact direct-profile
validation, `ClaudeExecutable`, `ClaudeRuntimeConfig`, and `ClaudeRuntimeProvider`; provider wire,
HTTP, subprocess, and credential types remain private.

The client sends `POST /v1/messages` with `anthropic-version: 2023-06-01`, a sensitive
`x-api-key`, JSON content, and an optional canonical `anthropic-beta` header. Construction owns the
hardened Reqwest/Rustls transport from `peritus-provider-core`, with redirects, ambient proxies,
and implicit HTTP retries disabled. Credentials are resolved immediately before every submission
and are absent from `Debug`, errors, and captured conformance observations.

## Contract

- Exact immutable profiles must use provider `anthropic`, dialect `AnthropicMessages`, stateless
  replay, unsupported cursor resume, and best-effort local cancellation. Unsupported lifecycle or
  capability claims fail configuration.
- Streaming must be selected during request negotiation because the wire request always uses
  `stream: true`. System instructions become top-level `system`; other roles, tool results,
  image/document sources, strict Draft 2020-12 tools and output schemas, cache breakpoints,
  sampling, and adaptive thinking are checked before credential resolution or transport.
- Thinking signatures and redacted-thinking data use bounded opaque replay events and are accepted
  on a later request only in the exact Anthropic-owned shape.
- HTTP 200 is accepted only with `text/event-stream` (parameters allowed). The incremental parser
  enforces the Messages event grammar, block order, bounded fragmented UTF-8 and tool JSON,
  cumulative usage, metadata, and an explicit terminal. Malformed data, interruption, premature
  EOF, unknown correctness-critical variants, and SSE `error` events fail closed.
- Connect/TLS failures proven pre-submission may follow the configured bounded retry policy.
  Explicit non-accepting 409/429/5xx responses may be recreated according to that policy. Any
  other transport failure is maybe-sent and becomes an ambiguous-acceptance terminal without a
  blind retry. Anthropic Messages exposes no create-idempotency, retrieval, server cancellation,
  or exact cursor-resume contract.

## Claude account runtime

`ClaudeRuntimeProvider` uses an already-authenticated, pinned official `claude` executable only as
a credential-owning model router. Peritus never reads OAuth tokens, Claude credential storage, or
keychain state, and it provides no login UI. Authentication is checked with exactly
`claude auth status --json`; an unauthenticated result tells the user to run `claude auth login`
outside Peritus.

One turn invokes the executable with `-p --output-format json`, the exact profile model, Peritus's
bounded `--effort` selection (high by default), `--safe-mode`, `--tools ""`, `--disallowedTools "mcp__*"`,
`--disable-slash-commands`, `--no-chrome`, `--no-session-persistence`, `--strict-mcp-config`, an
empty `--mcp-config`, a private `--system-prompt-file`, and a required `--json-schema`. It runs in a
fresh private directory and removes `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, and
`CLAUDE_CODE_OAUTH_TOKEN` from both status and turn processes. Peritus owns the complete transcript,
tool catalog, policy, tool execution, and cancellation lifecycle. Every prompt contains the typed
`peritus_tool_protocol` catalog and tells the model to return inert host requests through the
validated `tool_calls` field; Peritus executes them and replays each result on the next turn. Claude
native tools, plugins, MCP, slash commands, browser integration, and session persistence are not
part of this adapter.

The runtime accepts only text and inert host-tool history and normalizes one final structured result
into deterministic text/tool/usage events. Its honest profile advertises tool calls, bounded
parallel tool calls, portable reasoning-effort selection, and detailed usage. It is stateless, has no exact resume or remote
cancellation, does not advertise streaming, and marks output-token limits advisory because the
official executable exposes no exact `max_output_tokens` turn flag. Missing structured output is an
incomplete terminal; malformed JSON/schema fails as malformed; nonzero exit after partial stdout is
an interrupted incomplete terminal; an empty post-submit exit remains ambiguous. Cancellation kills
and reaps the owned process before emitting one cancelled terminal.

The runtime accepts automatic provider-managed prompt caching as a no-flag routing policy. It does
not accept explicit cache identities or TTL breakpoints, and Peritus retains the complete stateless
transcript locally.

The documented final-result envelope does not expose a trustworthy typed rate-limit or Retry-After
contract. Generic `is_error` results are therefore non-retryable provider terminals in production,
and the runtime profile leaves rate-limit detail unsupported. A2 separately qualifies Peritus's
provider-core planner with test-only explicit rejected rate/transient fixtures: the checked plan and
delay drive two real process turns. This proves planner/process composition without claiming that
production Claude errors can be classified from undocumented or untrusted result text.

### Live account qualification

After `claude auth status` confirms an authenticated official CLI, run the retained credentialed
probe from the repository root:

```text
CARGO_BUILD_JOBS=1 cargo run --locked --package peritus-release-qualification --example claude-live-account --all-features
```

An optional final argument overrides the default `sonnet` model. The probe goes through
`ClaudeRuntimeProvider`, not a direct CLI shortcut. It requires contiguous normalized events,
usage, an exact fixed canary, no tool activity, and a completed terminal. The command consumes
account usage and is intentionally not executed by credential-free Gate A.

## Fixtures and qualification

`fixtures/v1/MANIFEST` records the official contract sources and corpus purpose. `SHA256SUMS`
locks golden request, text/tool/thinking streams, malformed and incomplete streams, ancillary and
correctness-critical unknown events, an error after HTTP success, and HTTP auth/rate/transient
failures, plus authenticated/unauthenticated and structured runtime results. Tests verify every
digest, fragmented parsing, request projection, status/redaction classification, and retry safety.
The direct adapter runs all 14 `peritus.provider` A2 cases against an isolated loopback server. The
runtime runs the same 14 cases through the real Tokio process transport and a copied portable Rust
fake executable, including full auth/turn argv, private cwd, environment removal, cancellation,
reaping, and checked two-turn planner cases. The fake binary is available only behind the
`test-runtime-fake` feature in this unpublished crate; it is not a user CLI. No live Anthropic
credentials or API calls are used.

## Dependencies

Production code depends only on the Peritus model protocol, provider core, bounded foundation
types/codecs, Base64, Tempfile, and private Serde wire encoding. `peritus-conformance`,
`peritus-test-support`, and Tokio are development-only; neither the public API nor the production
provider surface exposes their types.

## Focused gates

From the repository root:

```console
CARGO_BUILD_JOBS=2 cargo test -p peritus-provider-anthropic --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-provider-anthropic --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' CARGO_BUILD_JOBS=2 cargo doc -p peritus-provider-anthropic --all-features --no-deps
just architecture
just source-layout
just ordinary-api
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-provider-anthropic --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
CARGO_BUILD_JOBS=1 cargo verus build --package peritus-provider-anthropic --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-provider-anthropic
```
