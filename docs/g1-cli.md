# G1 Scriptable CLI

G1 provides the `peritus` executable as the stable automation client for the protected local G0
daemon. It speaks A3 directly over the operating-system-local endpoint. It does not open a remote
TCP listener, inspect daemon storage, mint capabilities, infer committed work from connection
success, or duplicate domain reducers.

## Invocation and connection

```text
peritus [--endpoint <path-or-pipe>] [--session <32-hex-id>]
        [--timeout-seconds <positive-integer>] [--json] <command>
```

On Unix, the endpoint is a protected Unix-domain socket. On Windows, it is an owner-restricted
named pipe. `--endpoint` is required for every daemon command; help, version, and completion output
do not connect. Supplying `--session` asks G0 to resume that exact durable A3 session; otherwise
negotiation creates one. The daemon authenticates the operating-system peer and binds it to the
configured actor. A session identifier is continuity metadata, not a credential.

The client negotiates protocol version, required features, and limits before any application
request. Every later frame retains the negotiated session context. Heartbeats are answered while a
long-running stream is active, and read/write/request timeouts fail as connection or protocol
outcomes rather than being shown as success.

## Command surface

`peritus status` reports daemon readiness and diagnostic state. `peritus shutdown [--wait]`
requests orderly shutdown; `--wait` observes draining through its terminal clean or unclean result.

`peritus command submit` accepts exact B3 envelope and payload files:

```text
peritus command submit --actor <32-hex-id> --envelope command-envelope.bin \
  --payload command-frame.bin --idempotency-key <key> [--no-expected-revision]
```

The CLI validates and transmits the original frames and their binding metadata. G0 performs current
authority checks, domain dispatch, journal commit, idempotency resolution, and authoritative event
range reporting. A disconnect or pending disposition is never rewritten as committed work.

`peritus events watch` establishes an at-least-once subscription:

```text
peritus events watch --topic <topic> [--topic <topic> ...] [--after <cursor>] \
  [--window <positive-count>] [--count <positive-count>] [--snapshot-acceptable]
```

The client acknowledges cumulative cursors, removes identity-preserving redeliveries from output,
and reports a retention gap explicitly. It does not silently jump a missing cursor. `--count`
provides a bounded automation stop condition; without it the stream continues until cancellation or
daemon closure.

Artifact operations are `artifact get`, `artifact put`, and `artifact cancel`. Downloads validate
ordinal, offset, declared size, and SHA-256 before publishing the requested output path. Existing
files require `--force`. Uploads use a bounded configurable chunk size and cannot report completion
until G0 confirms the immutable artifact. Cancellation names both transfer and artifact identities.

Prompt operations accept the exact binding file returned by G0. A prompt answer contains exactly
one of a signed decision file, text, selection, confirmation, or secret reference, with an optional
rationale. The CLI treats all answers as protocol input. In particular, only a currently valid
externally signed B1 decision can authorize approval-gated work.

Terminal operations attach to a C2-owned process, follow ordered output, send bytes from a file or
standard input, resize the PTY, detach the client without killing the process, or explicitly request
process cancellation. Every control after attachment repeats the attachment, process, and
originating-request identities.

`peritus completions <bash|zsh|fish|powershell>` writes a deterministic completion program to
standard output.

## Stable output and exits

Human output is concise and intended for operators. `--json` emits one stable JSON object for a
single result and one object per line for streams and progress. Parse errors also honor a requested
`--json` flag, allowing automation to consume failures consistently.

Exit categories are part of the G1 contract:

| Code | Category |
| ---: | --- |
| 0 | Successful terminal outcome |
| 2 | Invalid command-line usage |
| 10 | Local connection failure |
| 11 | A3 negotiation failure |
| 12 | Typed daemon rejection |
| 13 | Local file or terminal I/O failure |
| 14 | Invalid or inconsistent protocol data |
| 70 | Internal client failure |
| 130 | User interruption |

## Verification

Focused local verification is:

```text
CARGO_BUILD_JOBS=1 cargo test --locked --package peritus-cli --all-targets --all-features
CARGO_BUILD_JOBS=1 cargo clippy --locked --package peritus-cli \
  --all-targets --all-features -- -D warnings
```

Parser tests cover closed commands, required values, duplicate options, IDs, bounds, and output
mode. Process-level tests cover help, version, completions, and stable exits. A3 codec, transport,
and daemon qualification remain owned by A3/G0 and their independent A2 suites.
