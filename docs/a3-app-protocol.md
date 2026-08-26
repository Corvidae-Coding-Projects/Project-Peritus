# A3 application protocol guide

## Purpose

`peritus-app-protocol` is the transport-neutral contract between the future Peritus daemon and its
CLI, TUI, and extension clients. It owns message identity, negotiation, canonical framing, bounded
request and event values, resumable delivery, streaming transfer rules, stable errors, generated
schemas, compatibility fixtures, and pure validation state machines.

A3 does not open a socket or named pipe. It does not authenticate a peer, acquire the daemon lock,
read a database, supervise a process, persist an idempotency result, or perform shutdown. G0 will
own those effects and use A3 values at its boundary.

## Wire families

All application frames use the existing PRTS format-version-one header and bounded canonical
primitive encoding. Application schema version one permanently allocates:

| Family | Name | Direction |
|---:|---|---|
| 94 | `app-client-hello` | client to server before negotiation |
| 95 | `app-server-hello` | server to client before negotiation |
| 96 | `app-request` | client to server after negotiation |
| 97 | `app-response` | server to client after negotiation |
| 98 | `app-event` | server to subscribed client |
| 99 | `app-control` | either direction for delivery/lifecycle control |

The codec rejects bad magic or format, nonzero flags, zero/foreign family and schema values,
truncation, trailing bytes, invalid UTF-8 or closed tags, length overflow, and every configured
limit violation. A successful decode establishes bounded syntax and semantic shape only.

## Version and feature negotiation

A client hello supplies canonical version ranges, required and optional protocol feature names,
and receive limits. The server selects the greatest common major and minor whose feature and limit
requirements can be satisfied.

The result is one of:

- `Compatible`: the preferred version, requested optional features, and limits were retained;
- `Downgraded`: a lower compatible version, feature subset, or tighter limit was selected; or
- `Incompatible`: no valid candidate exists, with a stable reason code.

`ProtocolFeatureName` uses A1's bounded canonical `CapabilityName` grammar, but it is negotiation
metadata and carries no B1 capability authority.

## Command requests and idempotency

A command submission binds:

- durable `SessionId`, claimed `ActorId`, `RequestId`, and `CorrelationId`;
- an opaque bounded `IdempotencyKey` and canonical request digest;
- an optional exact `RevisionTuple` expectation;
- the exact canonical B3 `CommandEnvelopeDto` frame; and
- the exact registered B3 command payload frame.

The expected revision must equal the revision inside the B3 envelope. The two frame byte sequences
and their SHA-256 digests are retained without reserialization. B3's family registry classifies
command and event roles so A3 does not copy domain DTOs or family lists.

The bounded idempotency model returns new admission, replay, key conflict, or capacity exhaustion.
The same actor/session/key and exact request fingerprint may replay the prior result. Changed reuse
is a conflict and cannot become new work.

A command result reports committed, replayed, or rejected status and always retains the original
request identity. Committed and replayed results require a checked nonempty event-cursor range;
rejected results require a stable application error. It is a transport observation, not a C0
`CommittedBatch` and not authority to claim that an event exists durably.

## Resumable subscriptions

An event cursor is a monotonic application position; zero is the origin before the first event. G0
will map durable journal positions into this namespace without exposing database implementation.

Each new delivery advances the cursor exactly once and carries a stable `EventId`, a distinct
delivery-attempt ID, and exact registered B3 event frame bytes/digest. Redelivery changes only the
attempt identity/count. Clients deduplicate by event ID.

Acknowledgement is cumulative. It may repeat or advance through delivered contiguous events; it
cannot regress, name another subscription, exceed delivery, or cross a retention gap. Acknowledged
events release in-flight credit. Once the negotiated window fills, the sender reports backpressure
and stops new delivery without discarding identity.

If the requested resume point is older than retained history, the subscription enters
snapshot-required state with requested, earliest, and latest cursors. Delivery does not silently
skip to the retained head. Pause, resume, and cancellation are explicit; cancellation is terminal.

## Artifact transfer

Artifact metadata identifies the transfer and `ArtifactId`, exact total bytes, canonical media
type, final `Sha256Digest`, and preferred chunk size within the negotiated ceiling. Chunks must have
matching identities, the next ordinal, the exact conserved offset, nonempty bounded data, and no
overflow beyond declared size.

Completion requires exact accumulated size and a matching digest supplied by an ordinary-Rust
incremental hash observation. Zero-sized artifacts complete without a data chunk. Cancellation and
failure are terminal. A3 completion does not claim C0 persisted or finalized the artifact.

## Approval and user input

Prompt bindings retain the originating request, prompt/session/actor target, `RevisionTuple`,
freshness digest, cancellation generation, and bounded choices or constraints. Answers must repeat
the full binding and match the caller-supplied live revision.

Approval answers are approve/deny/cancel intent only. G0 must authenticate the live peer and B1
must validate current action/revision authority and consume approval exactly once. User input is
bounded text, a selected option, confirmation, or a secret reference; A3 does not provide a secret
store.

## Terminal messages

A terminal attachment binds a `TerminalAttachmentId` to the C2-owned `ProcessId`. Output has one
monotonic message sequence and contiguous byte offset. Input is bounded, resize dimensions are
positive and bounded, and detach/cancel have explicit acknowledgement. Exit advances the sequence
once and is final; a second exit or output after exit is rejected.

The protocol preserves exact bytes. G1/G2 remain responsible for sanitizing terminal control
sequences before rendering.

## Readiness, diagnostics, heartbeat, and shutdown

Daemon status distinguishes starting, ready read/write, ready read-only, draining, and unavailable.
Read-only diagnostics never imply mutation readiness. Heartbeats carry stable identity and
monotonic sequence without pretending that local clock time is authenticated.

Shutdown request, accepted, draining/progress, and completed are separate facts. Completion reports
clean or unclean disposition and bounded remaining external work. Accepting a request is never
presented as completed shutdown.

## Stable errors

Every application failure separates:

- stable machine-readable `AppErrorCode`;
- retry disposition;
- responsible subsystem; and
- optional bounded diagnostic prose.

Clients branch on the first three fields, never the prose. The vocabulary covers negotiation,
malformed input, limits, session binding, idempotency, stale revisions, subscriptions,
acknowledgements, gaps, backpressure, artifacts, prompts, terminals, readiness, cancellation,
shutdown, and internal failures.

## Schemas and compatibility fixtures

Rust metadata owns family, payload, error, field, and bound allocation. The code generator emits:

- `app-protocol/generated/peritus-app-v1.schema.json`;
- `app-protocol/generated/peritus-app-v1.ts`; and
- `app-protocol/generated/peritus-app-v1.registry.md`.

Compatibility evidence uses A2's canonical layout:

```text
compat/app-protocol/v1/<case>/fixture.toml
```

Each case manifest classifies the fixture as minimal, realistic, corrupt, or adversarial and lists
every file with a lowercase SHA-256 digest. The generator's `--check` mode byte-compares all
generated outputs. Valid released bytes remain decodable; invalid fixtures remain rejected.

## Formal scope

Executable Verus predicates and proofs cover negotiation safety, monotonic cursor and legal ack
relations, redelivery identity, chunk conservation, completion size, terminal ordering, and
independent bounds. Ordinary-Rust refinement tests exercise the same predicates through public
constructors and state machines.

The proofs do not claim SHA-256 collision resistance, operating-system transport behavior, peer
identity, durable storage, scheduler fairness, PTY behavior, or daemon effects. Those observations
remain in their owning layers.

## Developer checks

Run focused work sequentially and keep the build resource bounded:

```text
CARGO_BUILD_JOBS=1 cargo test --package peritus-app-protocol --all-targets --all-features --locked -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --package peritus-conformance --all-targets --all-features --locked -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --package peritus-app-protocol --package peritus-conformance --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=1 cargo run --locked --package peritus-app-protocol --bin peritus-app-protocol-codegen -- --root . --check
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-app-protocol --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

After focused checks pass, `CARGO_BUILD_JOBS=1 just gate-a` is the complete local merge gate. The
hosted Linux, macOS, Windows, Foundation, and Verus matrices must also be green.
