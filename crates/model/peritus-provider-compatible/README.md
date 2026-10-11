# peritus-provider-compatible

Production adapter for endpoints that explicitly document one of two separately reviewed dialects:

- Responses-v1-compatible typed events; or
- Chat-Completions-v1-compatible `chat.completion.chunk` deltas.

The crate never infers feature parity from a model-list response, an endpoint name, or an
OpenAI-shaped URL. `CompatibleProfile` accepts only an immutable `CompatibleResponses` or
`CompatibleChatCompletions` protocol profile whose lifecycle and every supported capability have
an implemented mapping.

## Construction and public boundary

Create `CompatibleAuth` for bearer or an explicitly named API-key header, then pass one exact
operation `Endpoint` to `CompatibleConfig::new`. Fixed request headers must be bounded,
nonsensitive, non-routing values. Provider request-ID and rate-limit response headers are retained
only when explicitly named through `CompatibleResponseHeaders`; numeric reset units are part of
the declaration. Safe fixed query parameters may be embedded in the endpoint, while provider-core
rejects secret-bearing query names.

Bind the protocol profile with `CompatibleProfile::responses` or
`CompatibleProfile::chat_completions`, then call `CompatibleClient::new` with a Peritus
`CredentialSource`. The client owns its hardened Reqwest/Rustls transport internally. No Reqwest,
Serde/wire JSON, raw credential, transport handle, or provider SDK type appears in a public
signature. Credentials resolve after profile/request validation and immediately before each
encoded submission.

## Request and stream contract

Both mappings preserve ordered system/developer/user/assistant content, function definitions,
function calls and results, tool choice, bounded parallel-call selection, strict Draft 2020-12
structured output, HTTPS or inline images, sampling, output limits, and usage when the exact profile
and negotiated request declare them. Responses and Chat have distinct private JSON projections;
Responses rejects seed/stop controls that only the Chat mapping declares.

Streaming is mandatory and must be negotiated. Audio, documents, provider-file images, cache
controls, reasoning controls/replay, provider extensions, persistence, background work,
continuation, and any undeclared capability fail before credential resolution or transport.
Successful HTTP responses must be status 200 SSE and must follow the selected grammar with one
stable response ID and a mapped terminal. Typed Responses item/content indices and provider
sequences are checked. Chat content and tool argument fragments remain ordered text until their
finish. Bounded explicitly ancillary payloads are preserved as provider events; unknown output,
delta, item, finish, or terminal shapes fail closed.

## Lifecycle, retry, and cancellation

Compatible profiles are minimum-safe: stateless replay, no provider retention, no retrieval or
cursor resume, no background response, no confirmed server cancellation, and no create-idempotency
claim. Cancellation is a best-effort local abort of owned stream work and is normalized as an
unconfirmed cancellation terminal.

Only a proven pre-submission connect/TLS failure may be retried as not sent. HTTP 429 or 5xx may be
retried only when `CompatibleRetryStatuses` explicitly declares that class a temporary,
non-accepting rejection and the verified bounded `RetryPolicy` authorizes a fresh attempt.
`Retry-After` is honored within policy bounds. Any other transport failure may have sent bytes and
becomes ambiguous acceptance without a blind replay. Authentication, permission, invalid request,
not-found, conflict, undeclared 429/5xx, malformed success, and terminal stream failures do not
retry automatically.

## Failure and redaction policy

HTTP and stream failures normalize to typed provider-neutral events with static redacted diagnostic
codes. An explicitly mapped bounded provider request ID is preserved for correlation, but response
bodies and messages are not. Credential bytes, prompts, output, media, schemas, tool arguments,
raw SSE, and error bodies remain absent from `Debug`, errors, and captured observations.

## Fixtures, qualification, and dependencies

`fixtures/v1/MANIFEST` is the complete immutable corpus inventory and `fixtures/v1/SHA256SUMS`
pins every artifact except the digest file itself. The corpus covers both golden requests, realistic
text/tool/usage streams, fragmented delivery, exact duplicate sequences, ancillary and critical
unknowns, malformed and incomplete data, and authentication/rate/transient HTTP failures. Tests
verify inventory completeness and every digest.

Development qualification runs the production client against fresh deterministic loopback servers.
Its A2 bridge passes all 14 provider cases, including real HTTP 429/5xx recovery,
ambiguous-submission injection, interruption/cancellation, redaction scans, and separate selected
and foreign adapter instances. It never calls a live service or requires credentials.

Production dependencies are Peritus protocol/core/foundation crates, Base64, and private Serde JSON
encoding. `peritus-conformance`, `peritus-test-support`, and Tokio are development-only.

## Focused gates

From the repository root:

```console
CARGO_BUILD_JOBS=2 cargo test -p peritus-provider-compatible --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-provider-compatible --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' CARGO_BUILD_JOBS=2 cargo doc -p peritus-provider-compatible --all-features --no-deps --locked
just architecture
just source-layout
just ordinary-api
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-provider-compatible --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
CARGO_BUILD_JOBS=1 cargo verus build --package peritus-provider-compatible --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-provider-compatible
```
