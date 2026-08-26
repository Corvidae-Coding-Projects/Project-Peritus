# Feature: A3 Application Protocol Foundation

## Summary

A3 defines the complete transport-neutral application contract used by future daemon, CLI, TUI,
and extension clients. It adds `crates/app/peritus-app-protocol` as a verification-class-H hybrid
Verus/Rust crate with stable protocol identities, version and feature negotiation, canonical
request/response/event/control frames, bounded command idempotency, resumable at-least-once event
delivery, artifact streaming, approval and user-input correlation, terminal streaming, daemon
control messages, stable error codes, deterministic schema generation, compatibility fixtures, and
an executable A2 conformance catalog.

The protocol is deliberately below presentation and above transport. It knows nothing about Unix
sockets, Windows named pipes, peer credentials, daemon locks, SQLite ownership, worker supervision,
or process lifecycle. Those are G0 responsibilities. A3 defines the values and state-machine rules
that G0 must implement and that G1-G3 may consume.

Application command submission carries an exact bounded canonical B3 command frame. A3 does not
duplicate B3 command DTOs, deserialize app input into privileged objects, or claim that syntactic
validity grants authority. Actor identity, correlation, idempotency, and expected revision are
explicit request bindings; G0 must authenticate the actor and invoke the existing B0/B1/C0
authority and durability paths.

This is the production contract, not a prototype surface. Version-one schemas and fixtures become
immutable compatibility assets. Every collection and byte field is bounded, every closed tag is
checked, and the core safety relations are executable Verus specifications with ordinary-Rust
refinement tests.

## User-visible behavior

1. A client sends a bounded `ClientHello` declaring supported protocol versions, required and
   optional features, and receive limits. The server returns `Compatible`, `Downgraded`, or
   `Incompatible` with stable machine-readable reasons.
2. After a successful negotiation, every message binds the negotiated protocol session. Requests
   have stable request and correlation identities and receive exactly one typed terminal response,
   while long-lived work continues through subscriptions and control messages.
3. A command request binds the authenticated actor claim, idempotency key, request digest,
   correlation identity, optional expected `RevisionTuple`, and the exact canonical B3 frame bytes.
   Reusing a key with identical bytes replays the prior result; reusing it with different bytes is
   a stable conflict.
4. A command result reports `Committed`, `Replayed`, or `Rejected`. A committed/replayed result may
   report an exact nonempty event-cursor range, but the protocol value itself is not a C0 receipt and
   grants no authority.
5. Event subscriptions start after a cursor, deliver events at least once, use stable event IDs for
   deduplication, and accept cumulative acknowledgements only through delivered contiguous data.
   Retention gaps produce a typed snapshot-required result instead of silent skipping.
6. Backpressure is explicit. The negotiated in-flight limit, pause/resume controls, and last legal
   acknowledgement remain observable; a sender never signals successful delivery for discarded
   events.
7. Artifact transfer begins with exact metadata, then contiguous bounded chunks, then an exact
   completion record. Size, digest, media type, offset, ordering, cancellation, and completion are
   represented independently. A mismatch terminates the transfer as an error.
8. Approval and user-input prompts carry request, prompt, revision, and freshness bindings. Answers
   echo those bindings and remain user intent; they are not B1 approval authority until G0 verifies
   the live actor and current state.
9. Terminal attachment produces ordered output records, accepts bounded input and positive resize
   requests, and has explicit detach, cancellation, and exit. Output after terminal exit is invalid.
10. Readiness, read-only diagnostics, heartbeat, shutdown request/acceptance/completion, and protocol
    errors are typed messages. A shutdown request never implies that shutdown completed cleanly.
11. Rust schema metadata generates checked-in JSON Schema, TypeScript declarations, a classified
    compatibility corpus, and SHA-256 manifests deterministically on every supported host.

## Requirements

- **A3-R001 — Layer and ownership.** `peritus-app-protocol` is an app-layer, verification-class-H
  crate owned by A3. It depends only on required foundation contracts in production and on A2 in
  development.
- **A3-R002 — Transport neutrality.** Public APIs contain no socket, named-pipe, peer-authentication,
  database, worker, lock-file, or process-supervision implementation.
- **A3-R003 — Stable framing.** Six schema-v1 PRTS families are permanently allocated: 94
  `app-client-hello`, 95 `app-server-hello`, 96 `app-request`, 97 `app-response`, 98 `app-event`, and
  99 `app-control`.
- **A3-R004 — Stable identities.** Protocol, request, correlation, subscription, transfer, prompt,
  terminal-attachment, delivery, and heartbeat identities are typed nonzero 128-bit values. Domain
  `SessionId`, `ActorId`, `EventId`, and `ArtifactId` reuse A1 types rather than shadowing them.
- **A3-R005 — Explicit version range.** A version range has one nonzero major and an inclusive
  nonempty minor interval. Version collections are canonical, bounded, and duplicate-free.
- **A3-R006 — Deterministic negotiation.** Negotiation selects the greatest mutually supported
  minor of a common major, preferring the greatest major. Identical inputs produce identical
  results independent of insertion order.
- **A3-R007 — Feature semantics.** `ProtocolFeatureName` wraps A1 `CapabilityName` only for its
  proven bounded canonical grammar. Protocol features explicitly carry no B1 authority semantics.
- **A3-R008 — Required and optional features.** Every client-required feature must be supported at
  the selected version. Unsupported optional features are omitted and make the result downgraded;
  unsupported required features make it incompatible.
- **A3-R009 — Negotiated limits.** The result fixes the minimum mutually acceptable frame, string,
  collection, opaque-field, artifact-chunk, and in-flight-delivery bounds. Zero or internally
  inconsistent bounds are rejected.
- **A3-R010 — Negotiation outcomes.** Compatible, downgraded, and incompatible outcomes are closed
  typed values with stable reason codes. No prose is required for client branching.
- **A3-R011 — Session binding.** Every post-handshake frame carries the established protocol and
  durable session identities; mismatches reject before payload dispatch.
- **A3-R012 — Canonical outer envelopes.** Request, response, event, and control envelopes have
  closed payload tags, a single canonical field order, complete payload consumption, strict schema
  handling, and no unknown-field acceptance in version one.
- **A3-R013 — Request lifecycle.** A request ID is unique within a protocol session. A response
  echoes request and correlation IDs and is terminal exactly once.
- **A3-R014 — Command binding.** `SubmitCommand` binds actor, correlation, idempotency key, request
  digest, optional expected `RevisionTuple`, an exact canonical B3 `CommandEnvelopeDto` frame, and
  an exact B3 command payload frame. The outer expected revision must equal the B3 envelope revision.
- **A3-R015 — B3 frame identity.** Both B3 frames retain exact bytes, family, schema version, and
  SHA-256 digest. The envelope must be family 2/schema 1; A3 accepts only registered B3 command
  payload families and never reserializes either frame to establish identity.
- **A3-R016 — No DTO duplication.** A3 owns no copy of a B3 command/event/domain payload. It treats
  the validated command frame as bounded opaque canonical bytes until the owning subsystem decodes
  it.
- **A3-R017 — No false authority.** Decoding, negotiation, request admission, an actor claim, or a
  command result does not authenticate, authorize, consume approval, append an event, or prove
  durable commit.
- **A3-R018 — Bounded idempotency.** `IdempotencyKey` and exact request digest identify one command
  within an advertised bounded retention window. Same key/same digest replays; same key/different
  digest conflicts; capacity exhaustion is explicit and retry-classified.
- **A3-R019 — Command outcomes.** Stable command status is `Committed`, `Replayed`, or `Rejected`.
  Successful results identify the original request and optional exact committed event range;
  rejection carries a typed protocol error.
- **A3-R020 — Event-range validity.** A nonempty event range has positive first/last cursors,
  `first <= last`, and an exact checked count equal to `last - first + 1`.
- **A3-R021 — Subscription creation.** A subscription binds a stable ID, topic filter, origin or
  resume cursor, negotiated in-flight limit, and optional snapshot preference.
- **A3-R022 — Cursor semantics.** `EventCursor(0)` is origin and positive values are delivered
  positions. Within a subscription, newly delivered distinct events advance exactly and redelivery
  retains the original cursor and event ID.
- **A3-R023 — At-least-once delivery.** Lack of acknowledgement permits redelivery. Clients dedupe
  by stable `EventId`; delivery attempt IDs distinguish attempts without changing event identity.
  Each domain-event delivery preserves the exact registered B3 event frame and digest.
- **A3-R024 — Ack legality.** A cumulative acknowledgement may advance from the last acknowledgement
  only through a cursor already delivered contiguously. Regression, future acknowledgement, or an
  acknowledgement across a known gap is rejected.
- **A3-R025 — Gap handling.** If a requested cursor predates retained history or a contiguous range
  cannot be supplied, the server sends a typed gap with requested, earliest, and latest cursors and
  requires a snapshot/resubscription decision.
- **A3-R026 — Backpressure.** The sender cannot exceed the negotiated unacknowledged-event limit.
  Pause, resume, slow-consumer, and cancellation states are explicit and testable.
- **A3-R027 — Subscription cancellation.** Client or server cancellation is correlated, terminal,
  idempotent, and cannot be followed by new deliveries on that subscription.
- **A3-R028 — Artifact metadata.** Artifact metadata binds `ArtifactId`, exact byte size, bounded
  canonical media type, `Sha256Digest`, preferred chunk size, and transfer identity.
- **A3-R029 — Artifact chunks.** Each chunk binds transfer and artifact IDs, exact offset, nonempty
  bounded bytes, and ordinal. Chunks are contiguous, ordered, non-overlapping, and cannot exceed
  declared size.
- **A3-R030 — Artifact completion.** Completion is legal only when conserved chunk bytes equal the
  declared size and the observed digest equals metadata. Zero-sized artifacts complete without a
  data chunk. Digest computation remains an ordinary-Rust observation boundary.
- **A3-R031 — Artifact cancellation.** Cancellation is explicit and terminal. A cancelled or failed
  transfer cannot accept chunks or completion.
- **A3-R032 — Prompt correlation.** Approval and user-input prompts bind originating request,
  prompt identity, actor/session target, exact `RevisionTuple`, and a freshness digest.
- **A3-R033 — Prompt freshness.** Answers echo the complete correlation and freshness binding.
  Mismatch, stale revision, duplicate terminal answer, or answer-after-cancel is rejected.
- **A3-R034 — Approval intent only.** Approval answers encode approve/deny/cancel intent and bounded
  rationale. They do not encode a privileged B1 approval decision or signature.
- **A3-R035 — User input.** User input supports bounded text, selection, confirmation, and secret
  reference responses. Secret plaintext is not required by the protocol contract.
- **A3-R036 — Terminal attachment.** Attach binds a terminal attachment ID and owned process/action
  reference. Input, resize, detach, cancel, output, and exit use that exact attachment.
- **A3-R037 — Terminal ordering.** Output sequence and byte offset are monotonic and contiguous;
  resize dimensions are positive and bounded; exit is terminal; detach is idempotent; output after
  exit is invalid.
- **A3-R038 — Daemon lifecycle messages.** Readiness phase, read-only diagnostic state, heartbeat,
  shutdown request, shutdown accepted, shutdown progress, and shutdown complete are distinct typed
  payloads.
- **A3-R039 — Honest readiness.** `ReadyReadWrite`, `ReadyReadOnly`, `Starting`, `Draining`, and
  `Unavailable` remain distinct. A diagnostic response cannot be mistaken for mutation readiness.
- **A3-R040 — Honest shutdown.** Shutdown completion reports remaining externally active work and
  an exact clean/unclean disposition; request acceptance is not completion.
- **A3-R041 — Stable errors.** Every failure has a stable `AppErrorCode`, retry disposition,
  responsible subsystem, and optional bounded diagnostic prose. Codes and prose are independent.
- **A3-R042 — Error closure.** Version, malformed frame, limit, session, idempotency, stale revision,
  subscription, gap, artifact, prompt, terminal, readiness, cancellation, and internal categories
  have permanent v1 codes.
- **A3-R043 — Bounded canonical decoding.** Codec limits are fixed before allocation. Truncation,
  trailing bytes, nonzero flags, bad UTF-8, unknown schema/tag, duplicate/noncanonical collections,
  invalid option/boolean, overflow, and independent bound violations fail closed.
- **A3-R044 — Generic dispatch.** `decode_app_message` checks the PRTS header and dispatches only
  families 94-99. Typed decoders additionally require their exact family and schema.
- **A3-R045 — Deterministic schema metadata.** Rust-owned schema descriptors name every family,
  variant, field, bound, stable error code, and TypeScript representation in canonical order.
- **A3-R046 — Generated artifacts.** The codegen binary emits JSON Schema, TypeScript declarations,
  a human-readable protocol registry, classified binary fixtures, and SHA-256 manifests; `--check`
  rejects any drift.
- **A3-R047 — Compatibility corpus.** Version-one assets include minimal, realistic, corrupt, and
  adversarial cases with expected accept/reject code metadata. Valid fixtures round-trip byte
  exactly; invalid fixtures remain rejected by future compatible readers.
- **A3-R048 — A2 conformance.** A2 owns a runtime-neutral application-protocol subject contract and
  catalog covering negotiation, command binding/idempotency/results, resume/redelivery/dedupe/gaps,
  acknowledgements/backpressure, artifact transfer, prompt freshness, terminal ordering, lifecycle
  controls, malformed input, and independent bounds.
- **A3-R049 — Verus-first safety roots.** Negotiation, canonical semantic validation, cursor
  monotonicity, acknowledgement legality, chunk-size conservation, terminal ordering, and resource
  bounds have executable specs, proofs, and ordinary-Rust refinement tests.
- **A3-R050 — Proof honesty.** Formal claims exclude cryptographic collision resistance, OS
  transport behavior, peer identity, durable storage, scheduling fairness, and actual daemon/process
  effects. Those remain explicit G0/C0/C2 or observation boundaries.
- **A3-R051 — Maintainable decomposition.** `lib.rs` is a small documented facade. Production files
  target 400 lines and may not exceed 700 without a specific reviewed architecture exception. No
  `common`, `helpers`, `manager`, `misc`, or `utils` dumping-ground modules are introduced.
- **A3-R052 — Production evidence.** Focused ordinary tests, full resource-bounded Gate A,
  Linux/macOS/Windows CI, Foundation, Verus verify/build, and no-cheating checks all pass on the
  signed pull request before merge readiness.

## Acceptance criteria

1. `peritus-app-protocol` is registered as A3/app/H, appears in every exact Verus command inventory,
   and has only architecture-legal dependencies.
2. Public APIs construct and round-trip every hello, request, response, event, and control payload;
   generic dispatch rejects every foreign family or schema.
3. Negotiation exercises exact, downgraded, optional-feature, required-feature, disjoint-major,
   malformed-range, insertion-order, and negotiated-limit cases deterministically.
4. Command submission preserves exact B3 bytes/digest/family/schema and rejects non-command,
   unknown, oversized, malformed, or trailing B3 frames without creating a domain DTO.
5. The bounded idempotency model distinguishes new, replay, conflict, and capacity outcomes and
   never reports two effects for one key/digest pair.
6. Subscription traces demonstrate resume, same-event redelivery, client dedupe, legal cumulative
   acknowledgement, illegal future/regressive/gap acknowledgement, backpressure, gap/snapshot
   required, and terminal cancellation.
7. Artifact traces demonstrate empty and multi-chunk success, exact size/digest completion,
   out-of-order/overlap/overflow/oversized/digest-mismatch rejection, and cancellation.
8. Approval and user-input traces reject stale, mismatched, duplicated, and cancelled answers while
   preserving the distinction between client intent and B1 authority.
9. Terminal traces demonstrate ordered stdout/stderr output, input, resize, detach, cancel, exact
   exit, duplicate exit rejection, and output-after-exit rejection.
10. Daemon status, read-only diagnostics, heartbeat, shutdown acceptance/progress/completion, and
    stable protocol errors have canonical fixtures and cannot overstate readiness or cleanliness.
11. JSON Schema, TypeScript, protocol registry, fixture classification, and SHA-256 manifests are
    generated from Rust metadata, checked in, deterministic, and pass generator `--check`.
12. A2's application-protocol catalog is nonempty, publicly exported, inventory-tested, and passed
    by a production `peritus-app-protocol` subject without sharing implementation assertions.
13. `INV-023` through `INV-027` and `OBL-0189` onward identify real symbols and commands for the A3
    negotiation, delivery, transfer, terminal, canonical-validation, bounds, and refinement roots.
14. README, CHANGELOG, `docs/a3-app-protocol.md`, A2 inventory, architecture registry, formal
    inventory, manifests, command fixtures, and generated assets agree on the implemented surface.
15. Production source has no reachable placeholder success, `todo!`, `unimplemented!`, recoverable
    panic, unsafe code, ignored test, hidden transport/effect implementation, or god file.
16. `CARGO_BUILD_JOBS=1 just gate-a` and every required hosted check pass on signed commits before
    the pull request is reported mergeable. A3 is not merged by this work.

### Requirement traceability

| Requirements | Primary acceptance evidence |
|---|---|
| A3-R001-A3-R012 | Architecture checks, family/identity tests, negotiation matrix, codec dispatch tests |
| A3-R013-A3-R020 | Command-frame, idempotency, response, and committed-range tests |
| A3-R021-A3-R027 | Subscription model traces, A2 resume/redelivery/gap/ack/backpressure cases |
| A3-R028-A3-R031 | Artifact model traces, digest-boundary tests, corrupt/adversarial fixtures |
| A3-R032-A3-R040 | Prompt freshness, terminal traces, daemon status and shutdown cases |
| A3-R041-A3-R047 | Error-code inventory, codec matrix, generator checks, compatibility corpus |
| A3-R048-A3-R052 | A2 production conformance, formal inventory, Gate A, hosted PR matrix |

## Current architecture

- A1 provides nonzero 128-bit identifiers, `ActorId`, durable `SessionId`, `EventId`, `ArtifactId`,
  `RevisionTuple`, `Sha256Digest`, and the verified `CapabilityName` grammar.
- A2 provides runtime-neutral static conformance suites with subject adapters and stable case IDs.
  Its current placeholder `peritus.protocol` catalog must become the real A3 contract.
- B3 provides PRTS framing, bounded canonical reader/writer primitives, SHA-256 helpers, version-one
  domain DTOs, generated domain schemas, and a stable family registry through tag 93.
- B0-B3 and D0-F0 expose their command frames through `CanonicalEncode`/`CanonicalDecode`. Those
  exact frames are the only command-body authority A3 transports.
- C0 journal records expose one-based global positions and exact committed ranges, but A3 must not
  depend on C0. G0 will map durable positions to the app-owned cursor value.
- C2 owns process and PTY effects. A3 represents terminal intent and observations only.
- The app layer may depend on foundation and testing in development. `peritus-app-protocol` should
  not acquire state, runtime, orchestration, provider, UI, or daemon dependencies.
- The existing generator model uses Rust metadata plus a small deterministic filesystem driver with
  write and `--check` modes. A3 follows that model under A3-owned controlled roots.

## Proposed design

### Ownership and source layout

```text
crates/app/peritus-app-protocol/
  Cargo.toml
  README.md
  src/
    lib.rs
    limits.rs
    identity.rs
    version/{mod.rs,range.rs,feature.rs,negotiation.rs}
    envelope/{mod.rs,hello.rs,request.rs,response.rs,event.rs,control.rs}
    command/{mod.rs,binding.rs,idempotency.rs,result.rs}
    subscription/{mod.rs,cursor.rs,delivery.rs,ack.rs,gap.rs,backpressure.rs}
    artifact/{mod.rs,metadata.rs,chunk.rs,transfer.rs}
    prompt/{mod.rs,correlation.rs,approval.rs,user_input.rs}
    terminal/{mod.rs,message.rs,state.rs}
    daemon/{mod.rs,status.rs,heartbeat.rs,shutdown.rs}
    error/{mod.rs,code.rs,diagnostic.rs}
    wire/{mod.rs,semantic.rs,scalar.rs,hello.rs,request.rs,response.rs,event.rs,control.rs}
    schema/{mod.rs,registry.rs,render.rs,fixtures.rs,codegen.rs}
    verified/{mod.rs,negotiation.rs,delivery.rs,transfer.rs,terminal.rs,bounds.rs}
    bin/peritus-app-protocol-codegen.rs
  tests/
    negotiation.rs
    command_binding.rs
    idempotency.rs
    subscription_traces.rs
    artifact_traces.rs
    prompt_freshness.rs
    terminal_traces.rs
    daemon_controls.rs
    wire_matrix.rs
    generated_assets.rs
    production_conformance.rs
    verified_refinement.rs

app-protocol/
  generated/peritus-app-v1.schema.json
  generated/peritus-app-v1.ts
  generated/peritus-app-v1.registry.md

compat/app-protocol/v1/
  <case>/fixture.toml
  <case>/*.bin
```

The facade exports domain values and codecs by concern. Wire scalar functions stay crate-private;
they are not an alternate public model. Tests use public constructors except when testing decoder
rejection directly. No module combines all payload variants, codecs, transition state, and schema
rendering in one file.

### Frame families and dispatch

The six outer families are contiguous after B3's tag 93. Hello messages are separate because they
precede a negotiated protocol session. Post-handshake request, response, event, and control frames
carry `ProtocolId`, negotiated `ProtocolVersion`, and durable `SessionId`. Their semantic payloads
use closed `u16` tags allocated in per-family Rust registries.

`decode_app_message` first uses `peritus_codec::decode_frame`, then requires one of 94-99 and schema
one, then delegates to a typed payload decoder. `decode_message::<T>` remains available for callers
that already know the family. Both paths consume the complete payload. Unknown family, unknown
schema, and unknown semantic tag are distinct stable errors.

The A3 generator records every allocation. Existing tags and stable error codes are append-only
within a major version. A new optional field or feature requires a minor version and an explicit
compatibility bridge; removal, reinterpretation, or required-field change requires a new major.

### Identity and feature types

A private macro emits structurally identical newtypes backed by `[u8; 16]`, but each public type is
nominal and independently documented. Constructors reject all-zero bytes and expose exact bytes for
canonical encoding and Verus specifications. A3 adds only application identities; existing domain
identities are reused.

`ProtocolFeatureName` is a semantic newtype around `CapabilityName`. Its API exposes the canonical
name but no capability checks or authority operations. Feature collections are sorted by canonical
bytes, duplicate-free, and bounded. Version-one well-known names include event subscriptions,
artifact transfer, approval prompts, user input, terminal streaming, read-only diagnostics, and
graceful shutdown; unknown names may be negotiated only under a later compatible minor that
defines them.

### Negotiation

`ClientHello` carries the client protocol ID, supported ranges, required/optional features, receive
limits, and client implementation metadata bounded for diagnostics. `ServerCapabilities` is a local
input to the pure negotiation function and is not itself a wire authority. `ServerHello` echoes the
client protocol ID and contains a typed `NegotiationOutcome`.

The algorithm canonicalizes and validates inputs, walks common majors/minors from greatest to
least, rejects candidates missing a required feature, intersects optional features and limits, and
returns the first valid result. `Compatible` means both sides' preferred version and every requested
optional feature were selected without tighter-than-requested limits. Any otherwise successful
selection is `Downgraded`. No candidate yields `Incompatible` with a closed reason and diagnostic
details. The result fixes all later codec and flow-control bounds.

### Requests, command frames, and idempotency

`RequestEnvelope` contains protocol/session/request/correlation IDs and one closed `RequestPayload`.
Version one includes command submission, subscription start, artifact open/cancel, approval answer,
user-input answer, terminal attach/input/resize/detach, daemon status, and shutdown.

`CommandSubmissionFrames::parse` receives exact envelope and command bytes under A3 command limits.
It checks the PRTS headers, schemas, sizes, decodes the envelope only through
`CommandEnvelopeDto`, and requires the command payload's B3 registry role to be `Command`. It then
stores both exact frames and their SHA-256 digests without rewriting them. The B3 registry gains a
public closed `MessageRole` classification so A3 does not duplicate family lists.

`CommandBinding` combines those frames with actor, correlation, idempotency, request digest, and
expected revision. Construction requires the expected revision to equal the decoded B3 envelope
revision. Its request digest is domain-separated over the canonical outer command fields and both
exact frame byte sequences. It is an identity/integrity value, not authentication.

`IdempotencyWindow` is a bounded pure state model used by implementations and conformance tests. An
entry binds actor, key, request digest, original request, and final result. Admission returns `New`,
`Replay`, `Conflict`, or `Capacity`. Removal is explicit and ordered; A3 does not invent time or
durability guarantees. G0 must advertise and honor its retained capacity/retention policy.

`CommandResult` separates disposition from an optional `CommittedEventRange`. The range's
constructor checks arithmetic and contiguity. It reports only what the responding implementation
observed; C0's receipt remains the durable authority.

### Event subscriptions

`SubscriptionState` is a bounded pure state machine with origin/requested cursor, last delivered
cursor, last acknowledged cursor, in-flight count, pause state, and terminal state. A distinct new
event must be the successor of the last delivered cursor. A redelivery must reproduce event ID,
cursor, exact registered B3 event frame and digest, and subscription while incrementing only the
delivery attempt.

Acknowledgements are cumulative. They may repeat the current ack or advance through a delivered
contiguous cursor, but never regress, exceed delivered data, cross a declared gap, or target another
subscription. Acknowledgement releases the exact in-flight prefix. The negotiated limit gates new
delivery and produces explicit backpressure rather than truncation.

When retention cannot satisfy a resume request, `SubscriptionGap` reports the requested cursor and
the retained interval. The state becomes snapshot-required and cannot deliver ordinary events until
the client cancels or successfully creates a replacement subscription from an advertised snapshot.

### Artifact transfer

`ArtifactTransferState` starts from checked metadata. `accept_chunk` requires matching identities,
ordinal, expected offset, nonempty bounded bytes, and no overflow beyond declared size. The state
tracks conserved length only; it does not retain the whole artifact. Ordinary Rust may feed bytes
into an incremental SHA-256 observer and supplies the final observed digest.

`complete` succeeds only at exact declared size with matching digest. The zero-size case skips
chunks. Cancellation and failure are terminal and idempotent where the same terminal fact is
repeated. The protocol never claims that a completed transfer was persisted or finalized by C0.

### Prompt correlation and freshness

`PromptBinding` carries prompt kind, originating request, prompt/session/actor target, exact
revision, freshness digest, cancellation generation, and bounded choices/constraints. Approval
answers are unprivileged intent. User input is a closed value supporting text, selected option IDs,
confirmation, or an opaque secret reference.

`PromptState` accepts exactly one matching answer or cancellation. The caller provides the live
revision at admission; exact mismatch yields stale. This keeps the freshness predicate pure and
testable while leaving live state lookup and actor authentication to G0/B1.

### Terminal and daemon controls

Terminal requests are keyed by `TerminalAttachmentId` and bind the A1 `ProcessId` whose C2-owned
terminal is being observed. `TerminalState` checks monotonic output
sequence and contiguous byte offset, positive bounded dimensions, and terminal detach/cancel/exit
ordering. Stream bytes are bounded opaque fields; rendering and control-sequence sanitization belong
to G1/G2, while PTY/process ownership belongs to C2/G0.

Daemon controls model readiness and shutdown truth without implementing lifecycle. Heartbeats carry
nonce and monotonic sequence, not wall-clock claims. Shutdown progresses through requested,
accepted, draining, and completed observations. Completion contains a clean/unclean disposition and
bounded remaining-work descriptors.

### Errors, limits, and canonical validation

`AppProtocolError` contains `AppErrorCode`, `RetryDisposition`, `ResponsibleSubsystem`, and optional
bounded diagnostic text. Codes are stable kebab-case identifiers and fixed numeric wire tags. The
diagnostic is never parsed for control flow.

`AppProtocolLimits` refines `CodecLimits` with independent maxima for versions, features,
idempotency entries, topics, in-flight events, artifact chunk bytes, prompt choices, terminal chunk
bytes, diagnostic bytes, and remaining-work records. Constructors reject zero/inconsistent limits.
Every decode receives limits before any allocation.

Canonical semantic validators require sorted unique sets, exact enum tags, positive/nonzero fields
where specified, valid ranges, and complete payload consumption. Valid values re-encode byte
exactly. Corrupt input never becomes a partially trusted value.

### Schema generation and fixtures

Static Rust descriptors are the schema authority. Renderers emit canonical JSON Schema for the
documented JSON projection, TypeScript discriminated unions and branded IDs, and a registry
document containing binary family/payload/error allocations and bounds. Generation uses no network,
clock, randomized order, or host path.

The corpus uses A2's existing `compat/<surface>/<version>/<case>/fixture.toml` convention. Each
case manifest records its class, expected family, expected accept/reject code, and SHA-256 for every
payload file; no second competing digest inventory is added. It has four explicit classes:

- minimal valid hello/request/event/control values;
- realistic command, subscription, artifact, prompt, terminal, and daemon traces;
- corrupt frames for truncation, trailing bytes, bad header/schema/tag/length/UTF-8/digest;
- adversarial semantic frames for idempotency conflict, illegal ack/gap, chunk overlap/overflow,
  stale prompt, output-after-exit, and limit exhaustion.

`FixtureCatalog::load` and `verify_compatibility_coverage(RequireFixtures)` validate the complete
catalog. Generator tests compare memory output, checked-in bytes, per-case SHA-256 inventories, and
`--check` behavior.

### A2 conformance

A2 adds `application_protocol::{ApplicationProtocolScenario, ApplicationProtocolFixture,
ApplicationProtocolObservation, ApplicationProtocolSubject}` and `application_protocol_suite`.
Cases exercise the public behavioral contract without depending on A3 internals. The A3 production
subject translates each fixed fixture into real public values and reports direct observations.

The catalog contains separate cases for exact/downgraded/incompatible negotiation, required feature
failure, command identity, idempotent replay/conflict/capacity, event resume/redelivery/dedupe,
acknowledgement legality, gap/snapshot, backpressure/cancellation, artifact conservation/digest,
prompt freshness, terminal ordering, readiness/shutdown honesty, malformed frames, and independent
bounds. Stable suite/case IDs are inventory-tested in lexical order.

### Formal verification and governance

A3 registers five invariants:

- `INV-023 NegotiationSafety`: success selects a mutually supported version and all required
  features; incompatibility never contains a selected session.
- `INV-024 DeliverySafety`: delivered cursors are monotonic, redelivery preserves identity, and
  legal acknowledgements never exceed contiguous delivery.
- `INV-025 ChunkConservation`: accepted chunk lengths equal the tracked offset and legal completion
  equals declared size.
- `INV-026 TerminalOrdering`: output is contiguous and no output or second exit follows exit.
- `INV-027 ProtocolBoundedness`: successful construction/decoding satisfies every declared
  independent limit.

`OBL-0189` onward record executable negotiation, canonical validation, cursor, ack, chunk, terminal,
bounds, wire/refinement, generator, and A2 evidence. Proof roots live beside each concern under
`verified/`; ordinary modules call the proved predicates rather than maintaining a parallel rule.
Hash equality, UTF-8 library behavior, allocation, filesystem generation, and B3 frame decoding are
covered by ordinary-Rust refinement tests and explicit observation boundaries.

The crate is added to `architecture.toml`, Cargo manifests, `justfile`, Linux/macOS/Windows CI,
formal-governance workflows, xtask's exact Verus package lists, and every exact-command fixture.
A3-owned schema and fixture directories become controlled roots.

### Parallel implementation slices

| Work package | Exclusive write surface | Depends on | Completion boundary |
|---|---|---|---|
| A3.1 Core contract | crate manifest; identity, limits, version, errors, hello | signed design | negotiation and constructor tests pass |
| A3.2 Stateful flows | command, subscription, artifact, prompt, terminal, daemon | A3.1 public types | pure trace tests pass |
| A3.3 Canonical wire | `wire/` and family dispatch | A3.1-A3.2 type signatures | all six families round-trip/reject strictly |
| A3.4 Schemas/fixtures | `schema/`, codegen binary, `app-protocol/`, `compat/app-protocol/` | A3.3 | generated assets and `--check` pass |
| A3.5 A2/formal | A2 application catalog, `verified/`, verification manifests | stable A3 public APIs | production conformance and Verus roots pass |
| A3.6 Integration/docs | architecture/CI/xtask lists, README, guide, CHANGELOG | prior packages | focused checks and Gate A pass |

Only one worker edits a given surface. Public type signatures freeze after A3.1/A3.2 review; later
workers consume them. Shared manifests and generated assets are integrated last by one owner. This
permits parallel code production without competing edits or mutually inconsistent wire allocations.

## Data and compatibility

- Binary frames use the existing PRTS format and schema version one. Family tags 94-99 and semantic
  tags/error codes are permanent once merged.
- Valid version-one fixture bytes are immutable. A compatible reader must continue to decode them
  identically. Invalid fixtures must remain rejected with the same stable error category.
- Event cursor values are protocol positions, not database row IDs in the public contract. G0 must
  preserve monotonic durable mapping across restart for a subscription namespace.
- Idempotency retention is bounded and advertised; expiration/removal never permits the server to
  claim that an unknown old request was safely replayed.
- Artifact and B3 command payloads preserve exact bytes and digest. No compatibility step silently
  reserializes them.
- JSON/TypeScript representations are generated client projections, not the authoritative binary
  format. Their discriminants and branded identity shapes follow the same registry.
- Adding optional features or payload variants requires a minor-version compatibility decision and
  new fixtures. Removing or changing existing meaning requires a new major and migration bridge.

## Failure handling

- Constructor and state-machine failures use precise local error kinds; wire failures map to stable
  app error codes without losing codec category or offset in diagnostics.
- Negotiation failure produces only a server hello incompatibility. No post-handshake frame is
  admitted without a selected protocol/session binding.
- Idempotency conflict never executes as new work. Capacity exhaustion is explicit and cannot be
  hidden as transient success.
- Subscription gaps stop normal delivery and require a typed recovery action. Backpressure stops
  new delivery but preserves already delivered identity and ack state.
- Artifact mismatch, prompt staleness, terminal ordering violation, and cancellation transition to
  explicit terminal error states. No partial state is reported as complete.
- Unknown internal failures use a stable internal code and responsible subsystem; prose may include
  bounded context but never changes retry semantics.
- A3 performs no retry loop. It communicates retry disposition so G0 and clients can apply their
  own bounded policies.

## Security considerations

- All wire input is untrusted data and is bounded before allocation. Canonical decode never grants
  capability, approval, actor identity, durable receipt, or process ownership.
- Actor IDs in requests are claims. G0 must replace/verify them from authenticated peer context
  before B1 authorization.
- Protocol feature names are negotiation vocabulary, not B1 capabilities. No hierarchy or wildcard
  authority is inferred.
- Exact request/frame/freshness digests prevent accidental substitution and support higher-layer
  checks; they are not signatures or authentication.
- Approval responses are intent only. B1 authentication, currentness, action-digest binding, and
  approve-once consumption remain mandatory.
- Terminal and diagnostic bytes may contain hostile control sequences. A3 preserves bytes; G1/G2
  must sanitize rendering.
- Secret user input is represented by a reference where possible. A3 has no credential store and
  does not log or persist values.
- The design does not add remote transport, encryption, peer authentication, or daemon lifecycle
  policy. G0 owns those controls.

## Verification

Focused commands run sequentially with `CARGO_BUILD_JOBS=1`:

```text
cargo fmt --all -- --check
cargo check --package peritus-app-protocol --all-targets --all-features --locked
cargo test --package peritus-app-protocol --all-targets --all-features --locked
cargo test --package peritus-conformance --all-targets --all-features --locked
cargo clippy --package peritus-app-protocol --all-targets --all-features --locked -- -D warnings
cargo doc --package peritus-app-protocol --all-features --no-deps --locked
cargo run --package peritus-app-protocol --bin peritus-app-protocol-codegen --locked -- --root . --check
cargo verus verify --package peritus-app-protocol --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo verus build --package peritus-app-protocol --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

After focused review, `CARGO_BUILD_JOBS=1 just gate-a` is the local merge gate. The signed branch is
then pushed and a pull request opened. Linux, macOS, Windows, Foundation, Verus verify, Verus build,
and any repository policy jobs must all report success. Failures are fixed at their actual source
and the relevant focused/full checks rerun. The pull request is not merged by A3 work.

## Rollout and rollback

A3 ships as a library and checked-in compatibility contract; G0-G3 are not yet present to expose it
to users. Rollout consists of landing the crate, schemas, fixtures, A2 catalog, formal records, docs,
and build-matrix registration atomically on one reviewed pull request.

Before merge, rollback is branch/commit reversion. After merge but before a public release, a full
revert may remove A3 if no downstream code has landed. Once any release or downstream G0-G3 code
depends on version one, family tags, error codes, and valid fixture meanings are immutable; defects
are corrected additively with a minor version or, for semantic incompatibility, a new major and
explicit bridge. Generated assets are never hand-edited during rollback.

## Open questions

None. Family allocations, layer boundary, identifier ownership, feature semantics, cursor model,
command binding, compatibility policy, conformance surface, and formal evidence are fixed by this
design.

## Out of scope

- Unix-domain sockets, Windows named pipes, TCP, framing I/O, peer credential checks, ACLs, and
  transport encryption.
- Daemon composition, process singleton locks, startup reconciliation, storage migration, queue and
  outbox workers, supervision, crash recovery, and real shutdown orchestration.
- SQLite/journal implementation, durable idempotency storage, cursor retention storage, snapshots,
  and artifact persistence.
- PTY/process creation, input/output pumping, OS cancellation, and terminal rendering.
- CLI commands, TUI screens, extension/MCP host implementation, and generated runtime client SDKs
  beyond schema and TypeScript declarations.
- Authentication, authorization, B1 approval creation/verification/consumption, secret storage, and
  network security.
- Any claim that wire validity, a digest, an actor field, a command result, or a client acknowledgement
  is durable authority without the owning B0/B1/C0/G0 implementation.
