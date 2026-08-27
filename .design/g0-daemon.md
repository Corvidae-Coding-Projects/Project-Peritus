# Feature: G0 Daemon and Application Composition

## Summary

G0 turns the completed Peritus libraries into one production local daemon. It adds
`crates/app/peritus-daemon` as the H-class composition root and `peritusd` executable, owns the
single writable application instance, authenticates local IPC peers, applies the A3 protocol,
serializes authoritative state changes through C0, supervises bounded effect workers, recovers all
durable subsystems before mutation readiness, and shuts down without concealing active work.

This is a production slice, not a mock server or an in-memory integration demonstration. G0 closes
the concrete seams that only an embedding application can own: durable client sessions and
application idempotency, platform IPC, component construction, global event tailing, artifact
identity lookup, production recovery probes, workspace enumeration, outbox routing, credential
brokering, lifecycle supervision, diagnostics, and signal-driven shutdown.

The architecture verdict is **ready**. The preferred implementation is one serialized authority
owner plus bounded, owned effect workers. A shared journal mutex is rejected because it obscures
commit-before-effect ordering and invites synchronous locks across asynchronous work.

## User-visible behavior

- `peritusd serve --config <path>` starts exactly one daemon for a validated state root.
- Linux and macOS clients connect through a user-protected Unix-domain socket. Windows clients
  connect through a local-only, user-protected named pipe.
- Every connection is authenticated before an actor is bound, negotiates A3, and either creates or
  resumes a durable actor-owned application session.
- A healthy recovered daemon reports `ReadyReadWrite`. A startup failure that leaves durable state
  safe to inspect reports `ReadyReadOnly`; it never admits a mutation or effect.
- Commands are idempotent across reconnect and restart under actor/session/key identity, return the
  exact committed global event range, and never repeat a durable effect after ambiguous commit.
- Event clients resume from an application cursor, receive at-least-once delivery with stable event
  identity, acknowledge a contiguous prefix, and receive explicit gap/snapshot requirements.
- Clients stream artifacts in both directions with exact identity, size, media type, offset, digest,
  cancellation, and finalization behavior.
- Approval and user-input responses retain exact freshness and remain protocol input until the
  appropriate B1 authority path accepts them; approve and deny require the client's signed B1
  decision.
- Terminal clients attach to C2-owned PTYs, exchange ordered bytes and resize/cancel/detach
  controls, reconnect honestly, and receive one terminal exit.
- `SIGINT`/`SIGTERM`, Windows console shutdown, or an authorized A3 request drives the same bounded
  draining sequence. Clean completion is impossible while externally active work remains.
- Logs, status, trace, and telemetry output contain stable categories and redacted context rather
  than credentials, secret input, model content, terminal bytes, or untrusted repository text.

## Requirements

### Ownership and composition

- **G0-R001 — Slice ownership.** `peritus-daemon` is an app-layer, verification-class-H crate owned
  by G0. Its library exposes testable daemon planning and embedding surfaces; `peritusd` is the
  production executable.
- **G0-R002 — Single authority owner.** Exactly one `AuthorityOwner` owns the writable
  `SqliteJournal`, application ledger, state projections, durable subsystem drivers, and global
  outbox claim loop. No connection or effect worker opens an independent authoritative writer.
- **G0-R003 — Commit-before-effect.** Every effect is preceded by the exact existing subsystem
  authorization and durable directive. Effect observations return to the authority owner for a
  second durable settlement. No journal borrow, database transaction, or synchronous lock crosses
  an await or external operation.
- **G0-R004 — Existing authority.** G0 composes A3, B0-B3, C0-C7, D0-D3, E0-E3, and F0 through their
  public contracts. It does not duplicate their reducers, manufacture committed receipts, grant
  capabilities, waive quality, accept runs, promote harnesses, or infer effect success.
- **G0-R005 — Bounded ownership.** Every task, channel, queue, request, session, transfer,
  subscription, terminal attachment, retry, and shutdown wait has an explicit positive bound,
  owner, cancellation path, and observed terminal result.
- **G0-R006 — Component inventory.** Startup constructs one typed `DaemonComponents` inventory
  containing the exact store identity, backends, provider profiles, tools, project/workspace
  registrations, projection definitions, evaluator assets, and production harness pointer it will
  serve. Configuration cannot silently discover or enable authority-bearing components.

### Configuration, instance ownership, and platform IPC

- **G0-R007 — Strict configuration.** `DaemonConfig` has a version, canonical absolute state root,
  database/artifact/process/transaction roots beneath it, IPC endpoint, resource ceilings,
  configured projects/workspaces, one local human principal-to-actor binding, provider profiles,
  exporter policy, and credential references. Security-sensitive sections reject unknown fields
  and plaintext credentials.
- **G0-R008 — State-root safety.** Validation rejects empty, relative, aliased, overlapping,
  world-writable, symlink-replaced, or incorrectly owned authority paths. Creation uses restrictive
  permissions and reopens the resulting identity before accepting it.
- **G0-R009 — Exclusive instance.** A state-root lock records daemon instance identity, process
  identity, start observation, endpoint identity, and store identity. Live ownership returns an
  already-running result; stale ownership is removed only after an exact process/endpoint probe.
  RAII releases the lock after all writers and endpoints close.
- **G0-R010 — Unix transport.** Linux/macOS use a Unix-domain socket below the protected runtime
  root, reject non-socket/stale replacement, set user-only mode, compare accepted peer credentials
  to the configured daemon user, and unlink only the exact inode the daemon created.
- **G0-R011 — Windows transport.** Windows uses a local-only named pipe with a user/System-only
  DACL, remote-client rejection, exact accepted-client SID comparison, bounded instances, and
  handle ownership. The minimal Win32 descriptor/peer-query calls form an inventoried native TCB
  module with explicit safety invariants; deterministic admission remains safe Rust and Verus.
- **G0-R012 — No remote transport.** G0 contains no TCP listener, remote bind option, port
  configuration, or unauthenticated fallback. Adding a remote profile requires a later reviewed
  architecture amendment.
- **G0-R013 — Stream framing.** IPC reads the fixed 16-byte PRTS header first, validates magic,
  format, flags, and declared payload length before allocation, reads exactly the declared bounded
  payload, and passes one complete frame to A3. Truncation, timeout, trailing data within a frame,
  oversized length, and write failure close or reject the connection predictably.
- **G0-R014 — Peer-to-actor binding.** The live OS peer UID or SID maps through a configured,
  durable, one-principal-to-one-human-`ActorId` binding. G0 installs or verifies that binding before
  IPC readiness; changing either side requires an explicit offline configuration transition. A
  client-provided `ActorId` is only an equality assertion and never selects authority. Worker,
  model, and service actors do not authenticate through the human application socket.

### Protocol sessions and admission

- **G0-R015 — Durable session establishment.** A3 hello is amended before release so a client may
  request an existing `SessionId` and a successful server hello returns the established
  `SessionId`. G0 creates a new session or resumes one only after peer authentication and durable
  actor-ownership validation. Incompatible hello carries no established session.
- **G0-R016 — Session persistence.** Schema v10 stores session identity, actor, creation authority
  epoch, state, and last successful negotiation observation. Reconnection may change
  `ProtocolId`; it may not change session owner or resurrect a closed session.
- **G0-R017 — Exact context.** Every post-hello frame must carry the connection's exact protocol
  relationship, selected version, and established session. A mismatch is rejected before request
  dispatch.
- **G0-R018 — Negotiated limits.** The connection enforces the pointwise negotiated A3 limits on
  reads, writes, collections, pending requests, subscriptions, chunks, prompts, terminal data,
  diagnostics, and retained replay state.
- **G0-R019 — Request lifecycle.** One request identity is active at most once per negotiated
  connection. Responses echo request and correlation identity. Cancellation, disconnect, timeout,
  and daemon draining have explicit outcomes and do not erase durable work.
- **G0-R020 — Readiness admission.** `Starting` and `Unavailable` reject all application requests;
  `ReadyReadOnly` admits only hello, status, heartbeat, and explicitly classified read operations;
  `ReadyReadWrite` applies ordinary authorization; `Draining` rejects new mutations and permits
  bounded observation and shutdown progress.

### Durable commands and authority

- **G0-R021 — Application command ledger.** Schema v10 stores the unique
  actor/session/idempotency-key binding, complete A3 request digest, original request ID, B3 command
  ID, lifecycle state, exact committed range or stable rejection, and final response facts.
- **G0-R022 — Idempotency admission.** A missing key is new; the same key and digest replays the
  retained final result; the same key and different digest conflicts. A pending or indeterminate
  entry is reconciled by the same B3 command identity and digest before any replacement attempt.
- **G0-R023 — Atomic command completion.** The application-ledger pending record precedes dispatch.
  Successful B3 append and application completion become observably consistent under one checked
  settlement path. If SQLite cannot make both facts one transaction through current C0 APIs, the
  ledger retains `Indeterminate` and startup resolves C0 before writing the final application row.
- **G0-R024 — Freshness and authorization.** G0 compares outer and B3 revisions, authenticated
  actor, durable session, current authority epoch, policy, budget, lease, approval, and target
  ownership as required by the registered command handler. Decode and identity equality alone
  authorize nothing.
- **G0-R025 — Exact response range.** Only a real `CommittedBatch` creates a `Committed` result.
  Its positive `first_position` and `last_position` map exactly to A3 cursors. Reconciliation of the
  same batch creates `Replayed`; all pre-append refusals create a stable `Rejected` result.
- **G0-R026 — Closed dispatch.** G0 routes only B3 registry families with command role to explicitly
  registered handlers. Unknown, event-role, unsupported-schema, or unconfigured command families
  fail without an effect.

### Event subscriptions and outbox

- **G0-R027 — Global journal tail.** C0 gains a bounded public global-position range/tail query
  returning exact immutable records and retention bounds without exposing a raw SQLite
  connection. Cursor `n` is the C0 global position `n`; zero remains origin.
- **G0-R028 — Canonical topics.** G0 defines an append-only topic registry for all B3 event families,
  aggregate kinds, and stable system topics. Filters compile to a bounded deterministic predicate;
  arbitrary string-prefix authority is forbidden.
- **G0-R029 — At-least-once delivery.** Each live subscription uses A3 `SubscriptionState`, stable
  event IDs and frame bytes, distinct attempt IDs, strictly increasing source cursors, bounded
  in-flight windows, redelivery, cumulative acknowledgement of an actually delivered prefix,
  pause/resume, and explicit cancellation. Filtered-out source positions do not create false gaps.
- **G0-R030 — Resume and gaps.** Reconnect resumes strictly after the supplied cursor. A cursor
  before retained history or a missing contiguous position produces the exact A3 gap and
  snapshot-required behavior; G0 never skips to the current head silently.
- **G0-R031 — Ack authority.** Acknowledgements release only live delivery-window capacity. The
  journal remains authoritative and immutable; client acknowledgement cannot delete events or
  acceptance evidence.
- **G0-R032 — Single outbox router.** One bounded router claims C0 outbox rows in deterministic
  order with positive monotonic ticks and exact fences, dispatches by a closed destination
  registry, and acknowledges only the exact successfully settled claim.
- **G0-R033 — Outbox recovery.** Expired claims retry within their retained maximum; exhausted or
  unsupported destinations surface terminal diagnostics. A crash after effect but before ack uses
  destination idempotency/reconciliation and never assumes absence.

### Artifacts, prompts, terminals, and credentials

- **G0-R034 — Artifact catalog.** Schema v10 stores `ArtifactId` to immutable C0 digest, size, media
  type, availability, producing event, and reference identity. Conflicting reuse is terminal.
- **G0-R035 — Download.** `OpenArtifact` resolves the durable catalog, verifies C0 metadata/content,
  emits exact metadata, streams bounded contiguous chunks without loading the entire object, and
  finishes with the exact completion digest. Missing/corrupt content is explicit.
- **G0-R036 — Upload.** A3 is amended before release with begin-upload, upload-chunk, and
  complete-upload requests. G0 binds a temporary writer to actor/session/transfer/artifact,
  validates every ordinal/offset through `ArtifactTransferState`, finalizes C0 only at exact
  size/digest, then publishes the durable catalog fact. Cancellation or disconnect abandons the
  temporary writer for bounded recovery.
- **G0-R037 — Prompt routing.** Outstanding prompts are keyed by their complete A3 correlation.
  Answers and cancellations are accepted only from the authenticated actor/session at the current
  revision and cancellation generation. An approval prompt carries a bounded canonical B1 request
  plus the daemon-reserved decision command identity and registry revision. Approve and deny
  answers carry a canonical client-signed B1 decision; cancel remains an unprivileged cancellation.
  G0 strictly decodes and authenticates the signature against the decoded current durable registry,
  but never creates or holds a human signing key. Authenticated approval then follows B1 currentness
  and one-use consumption; user input stays bounded and secret references remain opaque.
- **G0-R038 — Terminal stream completeness.** A3 `TerminalStream` is amended before release with a
  combined PTY stream matching C2 `OutputStream::Terminal`; schema, TypeScript, fixtures, and
  compatibility tests change together.
- **G0-R039 — Terminal bridge.** Attach validates exact C2 process ownership and live terminal
  capability. One owned bridge forwards bounded input and resize, publishes globally ordered
  output with conserved byte offsets, handles detach without killing the process, propagates
  cancellation through C2, and emits one exit.
- **G0-R040 — Terminal recovery.** A client may reattach only to a live daemon-owned terminal
  whose exact process birth identity and output replay bounds are known. After daemon restart,
  C2 reconciliation reports live/absent/mismatched/indeterminate; G0 never pretends an
  unobservable PTY is reattached.
- **G0-R041 — Provider registry.** Configured direct and compatible C5 providers are indexed by
  immutable profile/revision. Account-backed Codex and Claude routes use their official executable
  adapters and leave login/token ownership in those executables.
- **G0-R042 — Credential broker.** G0 retains only credential/secret references and digests.
  Material is resolved through C3 after authorization and immediately before provider/native use,
  remains in zeroizing owners, and never appears in A3, logs, diagnostics, traces, state rows, or
  daemon configuration. B1 provides strict bounded decoders for its canonical approval request,
  signed decision, and credential-registry snapshot encodings. Startup decodes C0's exact current
  registry bytes and rejects any revision, generation, canonical-byte, or digest disagreement.

### Orchestration, observability, recovery, and shutdown

- **G0-R043 — Runtime ports.** G0 implements the existing production ports for D0 budgets/tools,
  D1 gate execution/recovery/evidence, E0 directive publication/child projections/acceptance, E1
  materialization, E2 analysis, E3 rollout execution, and F0 publication/activation using the
  already-authoritative subsystem APIs.
- **G0-R044 — Scheduler ownership.** D3 selects and reserves bounded work; G0 supervises the
  resulting worker task and reports observations. G0 does not invent scheduling priority,
  collaboration completion, reviewer independence, or acceptance.
- **G0-R045 — Production harness pointer.** Startup loads and verifies F0's current production
  pointer and exact E1 revision. Promotion and rollback still require F0/B0/B1/D2 authority; G0
  observes and applies only a committed activation.
- **G0-R046 — Trace and telemetry.** C7 receives causal redaction-safe lifecycle observations.
  G0 supports explicit `Disabled` and `LocalFile` production export modes. `LocalFile` writes
  redacted canonical batches to a protected bounded spool, atomically advances its checkpoint only
  after file synchronization, and recovers or quarantines incomplete batches on restart. Export
  buffering, acknowledgement, checkpoints, restart recovery, failure isolation, and shutdown are
  bounded. Export failure cannot fail authoritative work or be reported as success.
- **G0-R047 — Startup plan.** The canonical order is: validate config/paths; acquire lock; prepare
  endpoint identity; reconcile/apply migrations; open and verify C0; recover artifacts/evidence;
  rebuild projections; allocate authority epoch; restore B0-B3 state; enumerate/reconcile
  workspaces, leases, processes, sandboxes, secrets, tools, providers, D0-D3, E0-E3, and F0;
  reconcile application commands; resume outbox; bind/accept IPC; then publish readiness.
- **G0-R048 — Read-only failure.** A storage or integrity failure that permits safe diagnostics
  yields a bounded typed startup report and `ReadyReadOnly`; no mutation worker, provider, tool,
  outbox, or terminal bridge starts. An endpoint/identity failure that cannot safely serve even
  diagnostics yields `Unavailable` and process failure.
- **G0-R049 — Workspace enumeration.** G0 owns a durable/config-bound catalog of exact C1
  registration inputs. Startup enumerates it, opens each workspace through C1, reconciles pending
  mutations and holder quiescence, and reports per-workspace readiness without manufacturing a
  clean state.
- **G0-R050 — Production process probes.** C2 gains native Linux/macOS/Windows `ProcessProbe`
  implementations with exact birth-identity and owned-tree observations. Unsupported or
  permission-denied observation is `Indeterminate`, never `Absent`.
- **G0-R051 — Graceful shutdown.** Shutdown atomically enters draining, closes mutation intake,
  stops new dispatch, checkpoints queues, requests policy-selected cancellation or durable pause,
  joins owned tasks, settles possible observations, flushes storage/telemetry, closes IPC, and
  releases native resources and the instance lock in dependency order.
- **G0-R052 — Truthful completion.** Shutdown reports exact remaining requests, subscriptions,
  transfers, terminals, workers, processes, outbox claims, and indeterminate effects. `Clean` is
  constructed only when this set is empty.
- **G0-R053 — Kill recovery.** Forced termination at every meaningful startup, command, outbox,
  artifact, terminal, orchestration, and shutdown checkpoint restarts to an exact resumed,
  reconciled, read-only, or terminal-failure state without orphaned authoritative work.

### Verification and maintainability

- **G0-R054 — Verified decisions.** Verus covers lifecycle phase legality, readiness admission,
  startup ordering, session ownership, idempotency classification, cursor/range mapping, bounded
  queue accounting, outbox fence acceptance, artifact catalog/finalization predicates, and clean
  shutdown eligibility. Ordinary Rust refinement tests exercise the same public decisions.
- **G0-R055 — Honest TCB.** OS IPC, filesystem permissions, signals, async runtime, SQLite driver,
  process observation, and platform identity calls are effect boundaries. Any unavoidable unsafe
  symbol is minimal, separately reviewed, documented with safety invariants, and registered in the
  architecture/verification exclusion inventory with compensating platform tests.
- **G0-R056 — Stable errors.** `DaemonError` separates stable code, operation, retry/recovery class,
  responsible subsystem, and redacted source context. Public diagnostics never depend on prose
  matching.
- **G0-R057 — Decomposition.** `lib.rs` is a documented facade and `main.rs` only parses/constructs.
  Production modules normally remain below 400 lines and never exceed 700 without a reviewed
  architecture exception. Generic `manager`, `helper`, or `utils` god modules are prohibited.
- **G0-R058 — Production evidence.** Focused tests, A2 daemon conformance, protocol/schema fixture
  drift, migration upgrades, platform IPC tests, crash matrices, strict Clippy/rustdoc, formal
  verification, full resource-bounded Gate A, and the hosted Linux/macOS/Windows/Foundation/Verus
  matrix all pass before completion.

## Acceptance criteria

1. `peritus-daemon` is registered as G0/app/H, builds as a library and `peritusd` binary, appears in
   every exact formal command inventory, and contains no reachable stub or placeholder path.
2. A protected real Unix socket passes multi-client negotiation, authenticated peer admission,
   durable one-to-one peer/actor binding, malformed/oversized framing, reconnect, stale endpoint,
   second-instance, and shutdown tests on Linux/macOS; the equivalent named-pipe suite passes on
   Windows.
3. A successful hello establishes an exact durable actor-owned session; cross-actor resume,
   closed-session resume, context substitution, and incompatible hello are rejected.
4. Killing and reopening the daemon after pending, committed, rejected, ambiguous, and replayed
   command checkpoints produces exactly one command effect and the retained A3 result.
5. Schema v10 upgrades every prior fixture, installs application principals, sessions, commands,
   artifacts and required workspace catalog state, passes digest/recovery checks, and leaves
   existing C0 data unchanged.
6. Global event range/tail tests prove exact position/frame identity; black-box subscriptions prove
   resume, redelivery, dedupe identity, acknowledgement, pause, backpressure, gap, snapshot, and
   cancellation behavior.
7. Real artifact upload/download tests cover empty/small/multi-chunk/maximum-size content,
   mismatched IDs/offsets/digests, cancellation, disconnect, restart cleanup, missing content, and
   catalog conflict without partial authoritative publication.
8. Prompt tests prove actor/session/revision/generation freshness; strict request, signed-decision,
   and registry decoding; current-registry signature authentication; cancel behavior; and the
   absence of any unsigned approve/deny path.
9. PTY tests prove combined stream representation, exact sequence/offset conservation, resize,
   input, detach, cancel, exit, slow-client backpressure, and honest restart classification.
10. Fake and platform process-probe tests cover live exact identity, absent, PID reuse/mismatch,
    permission failure, owned descendants, and indeterminate recovery.
11. Production-port integration tests drive one complete writer -> gates -> review -> fixer ->
    acceptance scenario, one pause/resume, one cancellation tree, one rejected run, and one crash
    replay through the daemon owner without bypassing D/E authority.
12. Provider tests cover direct credential references, compatible profiles, Codex/Claude account
    routes, cancellation, retry, and redaction with no raw secret persisted or logged.
13. Outbox tests cover success, retry, expiry, stale fence, effect-before-ack crash, unsupported
    destination, exhaustion, restart, and bounded shutdown.
14. Telemetry tests cover disabled and local-file modes, redaction, synchronized checkpointing,
    incomplete-batch restart, quota exhaustion/loss diagnostics, exporter failure, and shutdown.
15. Startup failpoints at every G0-R047 stage yield the specified readiness and never start a
    forbidden effect worker. Recovery reruns are idempotent.
16. Shutdown tests from every readiness phase and with every active work kind prove intake closure,
    owned-task joining, durable recoverability, and no false clean completion.
17. The A2 daemon suite is runtime-neutral and a production black-box adapter passes every case
    without importing G0 internals.
18. Verus and refinement evidence maps every G0-R054 decision to exact symbols and commands; TCB
    inventories identify every effect/unsafe boundary without overstating proof.
19. `CARGO_BUILD_JOBS=1 just gate-a` passes locally after focused integration checks, then every
    required hosted runner passes and the signed PR is open, clean, and mergeable.

## Current architecture

### Existing authoritative boundaries

- A3 provides six closed PRTS families, negotiation, exact request/response/event/control
  envelopes, B3 frame binding, pure idempotency/subscription/artifact/prompt/terminal/daemon state,
  bounded codecs, schemas, fixtures, and A2 conformance. It deliberately performs no transport,
  peer authentication, session persistence, database work, or effect.
- C0 schema version nine provides `SqliteJournal`, immutable global event positions, command
  resolution by B3 `CommandId`, state records, exact outbox fences, integrity export, migrations,
  artifacts, evidence, and rebuildable projections. It lacks the richer A3 application ledger and
  a public bounded global tail query.
- C1 fully opens and reconciles one supplied registered workspace but does not enumerate daemon
  registrations.
- C2 owns process/PTY authorization, execution, output, control, persistence, and reconciliation.
  Its `ProcessProbe` intentionally requires a production OS observation implementation.
- C3 owns native sandbox/network/secret effects. C4 owns tool exposure and routing. C5 owns provider
  protocols/transports and official account executable routes. C6 owns context/memory/roles. C7
  owns durable traces and isolated telemetry projection/export.
- D0-D3 own the agent loop, gates, review, scheduling, and collaboration. E0 is the only delivery
  acceptance path. E1-E3 own harness materialization, debugging, and evaluation. F0 alone owns
  evidence-bound production harness activation and rollback.

### Existing data flow

```text
local client
  -> protected IPC / authenticated peer / A3 session
  -> bounded connection task
  -> AuthorityOwner queue
  -> existing verified authorization and reducer
  -> C0 append + state/outbox
  -> bounded effect worker
  -> observation back to AuthorityOwner
  -> C0 settlement
  -> A3 response/event
```

The `AuthorityOwner` is the only ordering point between application admission and durable domain
state. Connection tasks and effect workers never possess authority-bearing database handles.

## Proposed design

### Preferred design: serialized authority actor

`DaemonRuntime` starts one `AuthorityOwner` on an owned runtime task. It owns `DaemonState`, which
contains writable storage and reconstructed durable subsystem state. Bounded `mpsc` messages carry
typed `AuthorityRequest` values from IPC and effect supervisors; `oneshot` responses return exact
admission or settlement results. Requests never contain a privileged permit that can be reused by
another actor.

Effect work is represented by an owned `EffectTask` containing its durable directive identity,
destination, cancellation token, and join handle. `TaskSupervisor` uses a `JoinSet`, retains every
handle, observes panics/errors, and sends one typed outcome back to the owner. Shutdown closes
producers before joining consumers.

### Rejected alternative: shared journal mutex

Putting `SqliteJournal` in `Arc<Mutex<_>>` and letting connection/provider/tool tasks transact
directly is superficially smaller. It is rejected because task code can hold the lock across an
await, application ordering becomes scheduler-dependent, command and outbox settlement can race,
and shutdown cannot enumerate all authority users. Migration would later require replacing every
adapter. The serialized owner costs one bounded hop and preserves the current `&mut` C0 contract.

### Crate layout

```text
crates/app/peritus-daemon/
  Cargo.toml
  README.md
  src/
    lib.rs
    bin/peritusd.rs
    config/{mod,paths,profiles,limits}.rs
    error/{mod,code,recovery}.rs
    identity.rs
    instance/{mod,lock,record,probe}.rs
    ipc/{mod,frame,peer,endpoint,unix,windows}.rs
    session/{mod,ledger,negotiation,state}.rs
    startup/{mod,phase,plan,report,runner,reconcile}.rs
    storage/{mod,application,artifacts,workspace_catalog}.rs
    authority/{mod,owner,message,admission,settlement}.rs
    command/{mod,ledger,dispatch,recovery}.rs
    subscription/{mod,topics,registry,pump}.rs
    artifact/{mod,download,upload,registry}.rs
    prompt/{mod,registry,dispatch}.rs
    terminal/{mod,registry,bridge,recovery}.rs
    component/{mod,inventory,providers,tools,orchestration,evolution}.rs
    worker/{mod,queue,task,supervisor}.rs
    outbox/{mod,destination,pump,recovery}.rs
    telemetry/{mod,diagnostic,pump}.rs
    shutdown/{mod,plan,state,runner}.rs
    verified/mod.rs
  tests/
```

Small files may be consolidated when their combined responsibility remains cohesive and below the
normal line limit. `lib.rs` reexports stable embedding/configuration/status APIs, not internal
stores or native handles.

### IPC lifecycle

The endpoint accepts a stream only after OS peer checks. The stream reader reads PRTS header and
payload under pre-negotiation hard ceilings. The first frame must be `ClientHello`; any other
family closes with a stable error. Authentication happens before session selection. Successful
negotiation stores the exact `ConnectionContext`; request/control code compares every field.

One connection has separate bounded inbound and outbound queues owned by `ConnectionTask`. Slow
outbound consumers first pause subscription delivery, then receive backpressure, then are closed
after the configured bound. Command terminal responses are never silently discarded: a disconnect
leaves durable replay facts for the next session connection.

### Startup state machine

`StartupPhase` is closed and monotonic:

```text
Validate -> Lock -> Migrate -> Journal -> Artifacts -> Evidence -> Projections
  -> AuthorityEpoch -> DomainRecovery -> EffectRecovery -> AppRecovery
  -> Outbox -> Ipc -> ReadyReadWrite

any safe diagnostic failure -> ReadyReadOnly
unsafe endpoint/identity failure -> Failed
```

Each step returns a `StartupCheckpoint` with exact input/output digests and an explicit next
action. Recovery reruns pure classification before any effect. The daemon does not mark a stage
complete merely because a file or process exists.

### Protocol amendments

A3 remains transport-neutral but receives five pre-release completeness amendments:

1. `ClientHello` contains optional requested durable session; successful `ServerHello` contains
   the established session. Negotiation itself remains deterministic given the G0-selected session.
2. Requests add begin/chunk/complete upload variants using existing artifact types and state rules.
3. `TerminalStream` adds the combined PTY stream used by C2.
4. Subscription delivery cursors become strictly increasing source positions rather than requiring
   numeric `+1` between filtered deliveries. Acknowledgement remains cumulative over the exact
   delivered prefix, and a retention gap remains distinct from an intentionally filtered position.
   `SubscriptionState` therefore records a scanned watermark and an ordered delivered-cursor
   window; delivery proves `source_cursor > scanned_cursor`, while acknowledgement must name a
   member of that window and closes only its prefix. The A3 executable model and Verus obligations
   change with this rule.
5. Approval prompts carry a bounded opaque canonical B1 request frame, reserved B3 decision command
   identity, and credential-registry revision. Approval answers are either cancellation or a
   bounded opaque canonical B1 signed-decision frame; A3 checks correlation and bounds but never
   interprets the authority payload. B1 owns strict canonical request/decision/registry codecs,
   digest recomputation, signature verification, and currentness.

The generator updates JSON Schema, TypeScript, registry documentation, and every affected fixture.
A2's independent protocol contract gains matching cases. Because Peritus has no release and A3 was
merged solely to enable G0, these are schema-v1 completion changes rather than a compatibility
bridge. After G0 freezes the real transport contract, ordinary append-only compatibility applies.

### Schema v10 and application store

Migration v10 adds strict tables conceptually equivalent to:

```text
app_sessions(session_id PK, actor_id, authority_epoch, state, created_at,
             last_protocol_id, last_version_major, last_version_minor)
app_principals(principal_digest PK, principal_kind, actor_id UNIQUE, binding_digest, state)
app_commands(actor_id, session_id, idempotency_key, request_digest, request_id,
             command_id, state, first_position, last_position, error_code, result_digest,
             PRIMARY KEY(actor_id, session_id, idempotency_key))
app_artifacts(artifact_id PK, digest UNIQUE, byte_size, media_type, state, producing_position)
app_workspaces(workspace_id PK, registration_bytes, registration_digest, state)
```

All bytes are bounded by checks. Foreign keys bind sessions and producing journal positions where
appropriate. Application store methods expose typed transactions/classifications rather than raw
connections. The journal install schema and migration registry advance together with immutable
fixture upgrades.

### Command flow

1. Validate connection context, request uniqueness, actor/session binding, A3/B3 frames, and
   revisions.
2. Classify application idempotency. Return final replay/conflict immediately; reconcile pending
   rows by B3 command ID and request digest.
3. Record a new pending application row.
4. Dispatch through the closed domain handler registry. The handler performs existing B0/B1/B2
   decisions and creates a C0 append plan.
5. Append once. On success, settle the application row with exact positions. On ambiguous error,
   retain indeterminate and resolve. On pre-append refusal, settle a stable rejection.
6. Build an A3 response in the current connection context while preserving the original request
   identity inside `CommandResult`.

### Subscriptions and topics

C0's bounded global query returns `GlobalEventWindow { earliest, latest, records }`. G0 compiles
topics such as `event.<registered-family>`, `aggregate.<kind>`, and reviewed system topics to exact
predicates. One pump reads the next bounded range, filters without changing cursor truth, and uses
A3 delivery state. A delivery cursor is the selected record's global position even when intervening
records do not match. The subscription advances its scanned cursor separately from its last
delivered cursor so resume does not repeatedly scan unrelated history; A3 acknowledgement accepts
only a cursor that closes a prefix of deliveries actually sent on that subscription.

### Artifacts

C0's artifact store is refactored around owned streaming handles. `ArtifactReadHandle` owns its
verified file, immutable metadata, next offset, and incremental completion verifier.
`ArtifactWriteHandle` owns its temporary file, request, path identity, hasher, and byte count; it
does not borrow `ArtifactStore`. `ArtifactStore::complete_write(handle)` alone publishes the exact
prepared object and catalog row, which preserves single catalog ownership without a self-referential
daemon registry. Existing whole-buffer reads remain a convenience wrapper over the owned reader.

Downloads retain an `ArtifactReadHandle` and send chunks directly from disk under negotiated
limits. Uploads retain an `ArtifactWriteHandle` plus A3 transfer state in one owned registry entry.
The application catalog row becomes `Available` only after C0 finalization and the producing
journal transition; temporary/disconnected transfers remain non-authoritative and are removed or
quarantined on startup.

### Approval completion

G0 durably reserves the B3 command identity before emitting an approval prompt. B1's canonical
approval-request codec gives the client every signed semantic field without giving A3 authority
semantics. The client selects approve-once, deny, or an exact offered amendment, chooses an expiry
within the challenge bound, and signs with its external human credential. G0 accepts only the
prompt's reserved command identity, request identity/digest, authenticated human actor, permitted
role, current registry revision/generation, and fresh validity window. It decodes the current C0
registry snapshot through B1, verifies its exact stored bytes and digest, authenticates the signed
decision, then passes the resulting observation to B1/C0 settlement. An unsigned approve or deny
path does not exist.

### Telemetry export

`Disabled` records the explicit policy and starts no exporter task. `LocalFile` is the supported G0
production exporter: one bounded pump drains C7 batches into sequence-named files beneath the
protected telemetry root, synchronizes content and directory metadata, then advances a durable
checkpoint. Restart classifies complete, incomplete, and checkpointed files; quota policy retains
the newest unacknowledged batches and emits a stable loss diagnostic if a configured hard bound
requires eviction. Network OTLP export can be added later as another closed exporter and is not
silently simulated by this slice.

### Runtime ports

Adapters remain narrow:

- D0 budget and tool ports translate committed B1/C4 facts.
- D1 gate execution uses C4 quality tools and publishes via C0 artifacts/evidence.
- D2 review model work runs through independent D0 actors.
- D3 directives create owned worker tasks after durable reservation.
- E0 publisher routes exact committed directives and reads recovered child projections.
- E1 materialization invokes C1 only from a claimed directive.
- E2 model analysis and E3 rollout adapters use configured C5/C2/C3 boundaries.
- F0 activation applies only an already authorized `AtomicActivation` and verifies the resulting
  pointer.

No generic callback can append an arbitrary event or receive a capability token outside its target
adapter.

### Shutdown state machine

`ShutdownCoordinator` uses A3 `ShutdownState` and an internal ordered plan:

```text
Running -> Requested -> Draining
  -> intake closed
  -> subscriptions/prompts/transfers checkpointed
  -> orchestration pause/cancel requested
  -> workers and processes reconciled
  -> outbox claims settled/released by expiry
  -> telemetry checkpointed
  -> IPC closed
  -> storage closed
  -> instance lock released
  -> Clean | Unclean(remaining work)
```

The coordinator gathers remaining work from registries rather than trusting task counters. Forced
process termination leaves the same durable inputs for startup recovery.

## Data and compatibility

- Migration registry and journal install schema advance from nine to ten. Every prior fixture is
  upgraded; no reverse migration is claimed.
- A3 schema-v1 fixtures are regenerated for the pre-release session/upload/PTY/subscription/approval
  amendments. Their SHA-256 inventories remain exact and reviewed.
- G0 owns an append-only daemon topic registry and stable daemon error-code registry.
- Exact C0 event bytes remain unchanged. Cursor mapping uses stored global position and does not
  reserialize B3 frames.
- `peritusd` configuration version one rejects unknown security fields. Future additive fields
  require explicit defaults; authority-relaxing changes require a versioned migration.
- Platform endpoint names are derived from canonical state/store identity and do not expose project
  names or secret material.

## Failure handling

| Failure | Required behavior |
|---|---|
| Second live daemon | return already-running identity; perform no migration or endpoint replacement |
| Stale lock/endpoint | prove owner absent/mismatched, remove exact stale object, reacquire |
| Migration pending/ambiguous | reconcile exact operation; remain non-mutation-ready |
| Journal/store mismatch or corruption | read-only diagnostics or unavailable; never rewrite authority |
| Projection stale/corrupt | rebuild shadow from checked export and CAS install |
| Workspace/process indeterminate | quarantine target, report remaining work, do not infer absence |
| Command append ambiguous | resolve identical B3 command/digest; never issue replacement identity |
| Client disconnect | cancel connection-local work; retain durable command/effect truth |
| Slow subscriber | pause/backpressure then bounded close; retain journal truth |
| Artifact digest mismatch | fail transfer, discard/quarantine temporary, publish no catalog availability |
| Provider/tool/worker death | observe typed failure and settle through owning reducer |
| Outbox effect-before-ack crash | reconcile destination identity, then exact-fence acknowledge or retry |
| Telemetry export failure | retain/drop by explicit bounded policy and diagnose; do not fail authority |
| Shutdown timeout | report unclean exact remaining work and leave durable recovery inputs |

## Security considerations

- Local IPC is an authority boundary. Endpoint protection and peer identity are checked on every
  accepted connection; the durable principal binding selects the one human actor, and A3 actor
  values remain untrusted equality claims.
- Repository content, terminal bytes, model output, provider frames, prompts, artifact media types,
  and diagnostics are inert bounded data. None becomes a filesystem path, command, log format, or
  configuration key without its owning parser.
- The daemon runs as an ordinary user and refuses elevated/root operation unless a later reviewed
  deployment profile defines it.
- Native Windows named-pipe security requires minimal FFI. The module contains only descriptor,
  SID, pipe, and handle ownership operations; each unsafe call has a `SAFETY` argument and a
  platform test. No authority decision is inside unsafe code.
- Secrets are reference-only outside C3/C5 zeroizing scopes. Debug output is hand-written for
  secret-adjacent types.
- Diagnostic/read-only mode exposes bounded health and integrity categories, not raw database rows,
  configuration secrets, provider responses, terminal bytes, or artifact content.
- Rate and resource limits apply before allocation and before queue admission. A local same-user
  client cannot create unbounded retained state.
- G0 does not load native plugins, accept remote TCP, or expose MCP; those remain G3 and later
  qualification work.

## Verification

### Focused development checks

```text
CARGO_BUILD_JOBS=1 cargo test --package peritus-daemon --all-targets --all-features --locked
CARGO_BUILD_JOBS=1 cargo test --package peritus-app-protocol --package peritus-journal --package peritus-migrations --package peritus-process --package peritus-workspace --package peritus-conformance --all-targets --all-features --locked
CARGO_BUILD_JOBS=1 cargo clippy --package peritus-daemon --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=1 RUSTDOCFLAGS='-D warnings' cargo doc --package peritus-daemon --all-features --no-deps --locked
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-daemon --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

Only one Cargo or Verus workload runs at a time. Focused tests precede one final
`CARGO_BUILD_JOBS=1 just gate-a`.

### Evidence matrix

| Requirements | Evidence |
|---|---|
| G0-R001-G0-R006 | architecture/API checks, composition tests, owner/task inventories |
| G0-R007-G0-R014 | config/path/lock tests and real Unix/Windows IPC suites |
| G0-R015-G0-R020 | A3 fixture/codegen drift, session ledger and black-box negotiation/admission tests |
| G0-R021-G0-R026 | migration, command crash matrix, domain dispatch and exact-range tests |
| G0-R027-G0-R033 | global-tail, topic, subscription and outbox recovery tests |
| G0-R034-G0-R042 | artifact/prompt/terminal/provider/credential integration tests |
| G0-R043-G0-R053 | production-port scenarios, startup/shutdown/kill recovery and telemetry tests |
| G0-R054-G0-R058 | Verus/refinement inventory, TCB audit, lints/docs, Gate A and hosted matrix |

### A2 daemon conformance

The independent G0 contract includes at least: compatible and incompatible session establishment,
peer/actor mismatch, context mismatch, new/replay/conflict/indeterminate command, stale revision,
subscription resume/redelivery/ack/gap/backpressure, artifact download/upload/corruption,
prompt freshness, PTY ordering, read-only admission, second instance, startup failure, outbox crash,
graceful shutdown, forced restart, bounds, malformed frame, and non-authority cases.

## Rollout and rollback

G0 is not released independently. It lands behind no fake-success feature flag and becomes the
only production composition path used by G1-G3. Before a release consumes schema v10, rollback may
remove G0 and restore the pre-G0 branch/database fixture. After any real database is migrated,
rollback means restoring the pre-migration backup or running a compatible newer daemon; there is no
reverse SQL migration.

The signed design lands before implementation. Implementation may use internal commits, but the PR
must contain all protocol, migration, code, fixtures, tests, documentation, and verification
evidence together. The PR is not merged by this slice.

## Parallel implementation slices

| Package | Surface | Dependency | Completion signal |
|---|---|---|---|
| G0.1 Contracts | A3 session/upload/PTY/subscription/approval, B1 codecs, C0 tail/ledger/catalog and owned artifact handles, C1/C2 recovery APIs | signed design | focused contracts and migration tests pass |
| G0.2 Core owner | config, errors, identities, lifecycle, verified rules, authority owner | G0.1 signatures | pure/refinement and owner tests pass |
| G0.3 IPC/session | framing, peer auth, Unix/Windows endpoints, connection state | G0.1-G0.2 | platform black-box protocol tests pass |
| G0.4 Services | commands, subscriptions, artifacts, prompts, terminals | G0.1-G0.3 | service and crash tests pass |
| G0.5 Composition | providers, tools, D/E/F ports, workers, outbox, telemetry | G0.2 | production scenario tests pass |
| G0.6 Lifecycle | startup, recovery, shutdown, binary | G0.2-G0.5 | kill/restart matrix passes |
| G0.7 Integration | A2 suite, governance, docs, README, CHANGELOG | all prior | focused checks and Gate A pass |

Only one worker edits a given surface. Public contracts freeze after G0.1/G0.2 review. The primary
agent owns integration and all Cargo/Verus workloads.

## Open questions

None. The session, migration, cursor, platform IPC, ownership, recovery, unsafe-boundary, and
shutdown choices are fixed above for implementation.

## Out of scope

- G1's user-facing CLI command set, shell completions, JSON presentation, and stable client exits.
- G2's interactive TUI and terminal rendering/escape handling.
- G3's MCP server, plugin SDK/host, Wasm components, extension discovery, and extension trust UX.
- Remote TCP, hosted coordination, multi-user server deployment, and browser transport.
- OS installers, service-manager packaging, update distribution, and release signing owned by
  H2/H4.
- Final H0 security, H1 chaos, H2 platform, H3 performance/soak, and H4 release verdicts. G0 adds
  their required hooks and evidence but does not issue their qualification decisions.
- Redesign of existing B-F authority semantics. G0 supplies effectful adapters and durable
  composition only.
