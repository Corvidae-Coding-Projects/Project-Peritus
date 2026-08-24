# peritus-provider-google

First-party Google Gemini adapter with two explicit production dialects:

- preferred stable-v1 Gemini Interactions at `POST /v1/interactions`;
- stable-v1 Generate Content at
  `POST /v1/models/{model}:streamGenerateContent?alt=sse`.

It never uses `v1beta` or an SDK default. The reviewed contract is dated 2026-08-24 and follows
Google's official [Interactions API](https://ai.google.dev/api/interactions-api),
[Generate Content API](https://ai.google.dev/api/generate-content), and
[thought-signature](https://ai.google.dev/gemini-api/docs/thought-signatures) documentation.

## Construction and public boundary

Call `GoogleConfig::new` with a clean API origin, a `CredentialReference`, an exact immutable
`ProviderProfile`, and bounded HTTP/framing/retry policies. Then construct `GoogleClient::new` with
that config and a boxed Peritus `CredentialSource`. The client owns its hardened Reqwest/Rustls
transport internally; redirect following, ambient proxies, and implicit transport retries are
disabled. No Google SDK, Reqwest, Tokio, wire-JSON, transport, or raw-credential type appears in a
public signature.

Credentials resolve immediately before every encoded submission and are sent only as the
sensitive `x-goog-api-key` header. Requests are validated and encoded before credential resolution,
so unsupported or unnegotiated behavior stops before transport.

## Compatibility and lifecycle

Both dialects project system instructions, conversational content, functions and results, strict
Gemini-subset response schemas, image/audio/document input, sampling controls, and bounded streamed
events. Thinking controls are profile-gated; opaque thought signatures are replayed byte-exactly.
Generate Content supports explicit `cachedContent`; Interactions uses its documented implicit/state
model. Usage and cached-token counters remain cumulative/final observations, finish and safety
reasons remain typed, safe unknown ancillary events are preserved, and correctness-critical unknown
stream forms fail closed.

Generate Content is stateless. Interactions defaults to local-first `store=false`, may use semantic
continuation only under an exact provider-stored profile, and never offers exact cursor resume,
background retrieval, or confirmed server cancellation. Cancellation is a best-effort local abort
of owned connection/body work; the pull stream emits one explicit cancellation terminal and creates
no detached worker.

## Failure, retry, and redaction policy

HTTP authentication, permission, invalid-request, quota, rate-limit, cancellation, transient, and
provider failures normalize to redacted protocol failures. Bounded Google request IDs are retained
for operator correlation; response messages and bodies are not. A pre-submission connect failure may
follow the configured bounded fresh-request policy. An explicit non-accepting HTTP status may retry
when documented and policy-authorized, including bounded `Retry-After`. Any other maybe-sent
transport failure is reported as ambiguous acceptance and is never blindly recreated.

Credential bytes, prompts, outputs, reasoning state, media, function arguments, response bodies,
and raw SSE stay out of `Debug`, errors, and fake-server observations.

## Fixtures and qualification

`fixtures/v1/MANIFEST` records official source provenance and compatibility scope;
`fixtures/v1/SHA256SUMS` pins the request/stream/error corpus. It covers both minimal and realistic
dialects, text, tools, thinking, usage, corrupt/incomplete streams, ancillary and critical unknowns,
post-success SSE errors, authentication, quota/rate limiting, and transient failure.

Development tests run the production client against isolated owned loopback servers. The A2 bridge
creates a fresh subject for every one of the 14 provider cases, including real HTTP retry timing,
cancellation/interruption control points, redaction, and foreign-server isolation. No live API or
credential is used.

Runtime dependencies are limited to Peritus protocol/core/foundation crates, `serde`, `serde_json`,
and `base64`. Conformance, loopback support, and Tokio are development-only.

## Focused gates

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-provider-google --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-provider-google --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" CARGO_BUILD_JOBS=2 cargo doc -p peritus-provider-google --no-deps --all-features --locked
just architecture
just source-layout
just ordinary-api
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-provider-google --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
CARGO_BUILD_JOBS=1 cargo verus build --package peritus-provider-google --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```
