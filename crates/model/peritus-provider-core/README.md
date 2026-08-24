# peritus-provider-core

C5's provider-independent effect shell. It owns bounded HTTP values, endpoint and credential
validation, SSE/NDJSON framing, cancellation, deterministic retry execution, redacted diagnostics,
and object-safe transport/byte-stream interfaces.

Reqwest, Tokio, TLS, clocks, and asynchronous wakeups remain private implementation details; public
interfaces expose only Peritus-owned values and standard-library futures.

## Public surface

- `Endpoint` accepts only absolute HTTP(S) URLs without user information, fragments, traversal,
  backslashes, or known secret-bearing query fields.
- `CredentialSource` resolves an opaque `CredentialReference` immediately before request
  construction. `Credential` and sensitive header storage zeroize their owned buffers on drop and
  never reveal contents through `Debug` or errors.
- `HttpRequest`, `HttpResponse`, `HttpHeaders`, and the constituent method/status/header types own
  their data and enforce explicit `HttpLimits`. Caller-controlled connection, length, upgrade, and
  proxy headers are rejected.
- `HttpTransport` and `ByteStream` are object-safe and return `BoxFuture`, an alias containing only
  standard-library future types. A response always wraps its body in cumulative and per-chunk
  bounds, including bodies returned by custom transports.
- `ModelProvider` binds one adapter instance to an immutable `ProviderProfile`; requests are checked
  again at the effect boundary. `OwnedModelStream` owns cancellation, cancels live work on drop,
  and rejects end-of-stream before an explicit normalized terminal event.
- `CancellationToken` is cloneable, idempotent, race-free, and runtime-neutral at its public
  boundary. Backoff, connection, response-header, and body-read waits observe it directly.
- `SseParser` and `NdjsonParser` accept arbitrary byte fragmentation, including split UTF-8. They
  enforce buffer/frame bounds; SSE preserves comments, multiline `data`, event/id fields, and the
  exact `[DONE]` sentinel.
- `RetryPolicy` makes retry legality explicit across not-sent, explicitly rejected, ambiguous,
  accepted-without-events, partial-stream, and terminal states. A typed `Connect` failure proves
  that connection/TLS setup failed before submission; other transport failures remain ambiguous.
  Exact recreation or resumption requires the corresponding documented protection after possible
  provider acceptance.
- `Diagnostic` exposes only stable categories, phases, status, bounded content type, redacted
  provider request identity, byte count, and elapsed time. It never contains bodies or raw headers.
- `ProcessExecutable`, `ProcessRequest`, `ProcessLimits`, `ProcessOutput`, and `ProcessTransport`
  form the bounded account-runtime effect shell. Executables are canonicalized and pinned;
  argv/stdin/cwd/environment removals are explicit; debug output redacts argv, stdin, stdout,
  stderr, and executable paths. `TokioProcessTransport` is the production implementation, but
  Tokio process types remain private.

The model-provider interface accepts and emits the protocol crate's normalized values directly.
Provider wire structs and transport handles remain private to adapter implementations.

## Production transport policy

`ReqwestTransport` uses Reqwest with the Rustls backend and platform certificate verification.
Automatic redirects, implicit retries, ambient/system proxies, and referer forwarding are disabled.
The caller must supply a validated endpoint and an explicit retry plan. Sending and body reads are
cancellation-aware and do not create detached workers.

HTTP is accepted so deterministic loopback fake servers can exercise adapters. Composition policy,
not provider-core, decides whether a configured production endpoint must use HTTPS. Custom TLS
roots, certificate bypass, proxy inheritance, and cross-origin redirect policy are intentionally
unsupported in this stage.

Account-runtime subprocesses use piped bounded I/O, an explicit wall-clock deadline, and
`kill_on_drop`. Cancellation, timeout, output-limit, pipe, and wait failures all initiate child
termination and await reaping before returning. Tests exercise the public fake seam and the real
Tokio transport with a portable Rust helper, including argv/stdin/cwd/environment removal,
output bounds, cancellation, and process ownership on Windows and Unix.

## Failure and compatibility notes

Errors contain a stable `ProviderCoreErrorKind`, code, static operation, and static redaction-safe
detail. Transport-library error strings are not propagated because URLs and provider-controlled
text can contain sensitive data. Body and framing failures are terminal for that owned stream.

The crate deliberately exposes no Reqwest, Tokio, `url`, `bytes`, `zeroize`, or provider SDK type.
Those dependencies are implementation details and may change without altering adapter-facing APIs.
