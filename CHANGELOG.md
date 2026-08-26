# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Implement complete production E1 Harness Materialization (#20)
- Deliver complete production E1 harness materialization as the H-class `peritus-harness` crate,
  turning reviewed harness source into checked immutable revisions and exact durable workspace
  candidates without adding evaluation or promotion authority (#20)
- Add a complete closed catalog of thirty component kinds spanning instructions, roles, tools,
  middleware, skills, collaboration, memory, gates, orchestration, providers, observability, and
  evolution definitions, plus compiled protection for security roots, human authority, sealed
  evaluators, trust boundaries, and production-promotion rules
- Add strict schema-v1 `.peritus-harness/manifest.toml` parsing and C1 no-follow recursive loading
  with exact declaration/inventory equality, source/target confinement, byte-size and SHA-256
  verification, opaque binary component support, unknown-field rejection, and independent bounds
- Add typed component IDs, owners, provenance, media types, source/target paths, schema intervals,
  provider/platform feature requirements, dependencies, optional executable artifact identities,
  and canonical private-field constructors
- Add deterministic complete graph validation for duplicate/missing/self/cyclic dependencies,
  required kind/schema/digest and feature compatibility, protected dependency legality, canonical
  topological order, graph identity, and exact artifact-root projection
- Add closed descriptive authority sets with compiled per-kind ceilings and transitive dependency
  checks, while keeping actual effect authority exclusively in B1 and the target-owned gateways
- Add domain-separated content-addressed genesis and successor revisions whose identities bind the
  complete manifest, graph, declaration, content, provenance, compatibility, authority, path, and
  executable-artifact state
- Add an append-only bounded branched harness-history DAG with stable lineage identity, exact
  predecessor/number checks, ancestry queries, deterministic canonical snapshots, and no mutable
  revision API
- Make every protected controlled asset structurally immutable across successors: addition,
  removal, rename, reorder, content, schema, owner, provenance, dependency, compatibility,
  authority, path, and executable binding drift are all rejected
- Add deterministic materialization plans that bind an exact harness revision and C1 workspace
  snapshot, canonical create/replace operations, and deletes limited to paths proven owned by the
  exact prior E1 receipt, preserving every unrelated workspace path, with compiled file/count/byte
  ceilings fixed to the sole atomic C1 patch boundary
- Add bounded verified finalized-artifact reads to C0 and use exact returned bytes to construct one
  C1 `PatchSet`, expose deterministic inert patch/predicted-candidate authorization payloads, then
  perform separately authorized `WorkspaceGateway` patch and candidate creation
- Add complete materialization receipts retaining plan, patch, action, prior/current workspace,
  Git commit/tree, C1 manifest artifact, output inventory, rollback reason, and canonical identity
- Add ancestor-only rollback through the normal materialization pipeline, producing a fresh C1
  candidate and receipt without rewriting history, deleting descendants, or moving a production
  harness pointer
- Add a closed E1 command/event/state reducer with commit-before-effect planning, stable outbox
  directives, command idempotency, artifact dependencies, complete checkpoints, typed failures,
  and restart reconciliation for untouched, exactly completed, stale, and conflicting targets
- Add rebuildable read-only harness projections exposing immutable lineage/branches, graph and
  protected summaries, pending materialization, delivery state, receipts/failures, rollback
  ancestry, and artifact roots without mutation or promotion methods
- Add canonical schema-v1 harness command/event/state families 79-81 with strict tag, length,
  canonical-order, truncation, and trailing-byte rejection plus immutable compatibility fixtures
  and SHA-256 inventories
- Extend C0 with permanent `Harness` aggregate tag 13 and checkpoint namespace `0xE101`; add schema
  version six with a required-backup constrained-table copy that preserves every v5 tag 1-12 row
  and frame byte, admits E1, and verifies exact v5 restoration
- Add a narrow C0 append-time outbox acknowledgement mutation so an E1 success or failure event,
  complete checkpoint, and the exact claimed directive fence settle in one transaction
- Extend A2 with fourteen nonempty E1 scenarios covering manifest inventory, complete component
  catalog, graph/authority rejection, protected immutability, content-addressed history, forward
  and rollback materialization, artifacts, independent bounds, replay/idempotency, malformed
  frames, panic containment, and teardown isolation
- Add executable Verus specifications and proof-facing reference tests for component uniqueness,
  graph order and acyclicity, compatibility, authority non-widening, protected invariance,
  append-only ancestry, rollback confinement, materialization ownership, and replay equivalence
- Add the signed E1 design freeze, crate and operator documentation, architecture/formal/CI
  inventories, generated protocol metadata, resource-aware single-job hosted builds, measured
  thirty-minute Verus runner budgets, and current repository development-state guidance

- Deliver D3 scheduling and E0 AcTor orchestration (#19)
- Deliver production D3 scheduling/collaboration and the E0 AcTor delivery orchestrator as three
  focused H-class crates, composing the existing B0-B3, C0, C6, and D0-D2 boundaries without
  introducing another provider, process, workspace, policy, waiver, or acceptance authority (#19)
- Add a durable bounded D3 scheduler whose immutable binding, checked identities, compiled limits,
  resource vectors, worker descriptors, work specifications, dependencies, and recovery policy
  make every admission and dispatch decision explicit
- Add deterministic dependency readiness and feasible worker selection ordered by bounded bypass,
  priority, enqueue sequence, work identity, and worker identity, independent of wall time, map
  iteration order, task wakeups, or result arrival order
- Add exact reservation ownership with checked capacity addition/subtraction, one live work and
  worker owner per dispatch, acknowledgement-before-execution observation, conservative worker-loss
  classification, retry ambiguity, and capacity-preserving release
- Add bounded pause, drain, retry, dependency-failure propagation, cancellation-tree processing,
  worker loss, terminal quiescence, and truthful scheduler completion without implicit success
- Add a durable causal D3 collaboration aggregate with one acyclic depth-consistent task tree,
  explicit delegation offer/accept/activation, stable ownership, bounded fan-out and depth,
  canonical messages, exact artifact handoffs, and declared all-required/any-required joins
- Preserve actor, role, scheduler work/reservation, revision, parent, message, artifact, evidence,
  and causal predecessor bindings through every collaboration transition so an inert handoff cannot
  widen authority or detach work from its owner
- Add collaboration pause/resume and descendant-first cancellation whose durable pending work must
  settle before terminal cancellation; late success cannot resurrect a cancelled ancestor or
  manufacture aggregate completion
- Add closed scheduler and collaboration command/event/state reducers, complete rebuildable
  projections, exact sequence/predecessor replay, command idempotency, conflicting-command
  detection, complete checkpoint equality, and restart-safe C0 persistence
- Add canonical schema-v1 scheduler families 70-72 and collaboration families 73-75 with strict
  tag, bound, truncation, noncanonical-value, and trailing-byte rejection plus immutable binary
  corpora and SHA-256 manifests
- Extend C0 with permanent `Scheduler`, `Collaboration`, and `Orchestrator` aggregate tags 10-12;
  add schema version five with backup-required constrained-table copying that preserves every tag
  1-9 row and frame byte-for-byte while admitting the complete D3/E0 range
- Extend A2 with nonempty scheduler and collaboration scenario catalogs covering fair selection,
  dependencies, reservations, resource conservation, worker loss, retries, replay, delegation,
  joins, handoffs, cancellation, malformed input, and panic/teardown behavior
- Add executable Verus specifications and proofs for scheduler capacity conservation, bounded
  bypass, dependency readiness, terminal quiescence, replay claims, collaboration causality, join
  truth, cancellation dominance, pending-directive exclusion, and replay equivalence
- Add the complete D3 design and operator guide, architecture registration, formal obligations,
  generated protocol metadata, strict no-cheating inventories, focused domain/durability/replay
  tests, and resource-aware build guidance

- Add the production E0 `peritus-orchestrator` aggregate for the closed writer -> gates -> reviewer
  lifecycle and the sole review -> fixer -> new revision -> fresh gates correction loop
- Bind every E0 run to the exact B2 acceptance contract, B0 run/attempt, D1 plan, D2 policy,
  initial revision, D3 scheduler/collaboration identities, explicit service/writer/fixer/reviewer
  ownership, and independently bounded completion policy
- Add complete candidate bindings covering workspace snapshot, candidate/tree/artifact identities
  and digests, producer actors and ancestry, and a canonical binding digest; material change creates
  a full successor revision and invalidates all earlier D1/D2/B2 acceptance evidence
- Make writer completion install the actual changed candidate together with its checked
  same-revision D1/D2 quality-cycle binding while retaining the already-active D3 identities, so
  the normal path never pre-binds an unknown writer output
- Add canonical role handoffs that retain source/destination phase, actor and role, exact current
  candidate, D3 task/work ownership, input artifacts/evidence, and stable idempotency identity while
  excluding hidden reviewer reasoning from fixer inputs
- Consume checked D0 completion, D1 terminal/evidence, D2 quorum/finding/oscillation, D3 ownership,
  and B0 lifecycle observations through their public projections instead of reimplementing or
  weakening those authorities inside E0
- Add independently bounded writer, fixer, gate, review, revision, handoff, child-directive,
  retained-observation, artifact-reference, event/state-size, repeated-finding, and cancellation-
  reconciliation counters with exact `Rejected`, `Failed`, `Exhausted`, `NeedsHuman`, and
  `Cancelled` terminal causes
- Make `AcceptanceCertificate::from_evaluation` the only E0 certificate constructor, requiring the
  exact current B2 `AcceptanceEvidence` and acceptable `AcceptanceDecision`; require a matching
  durable B0 `AcceptanceAccepted` event before E0 can enter `Accepted`
- Add one-at-a-time commit-before-effect directives with stable destination/payload identity,
  bounded delivery state, durable acknowledgement, exact child-head observations, and explicit
  deliverable/awaiting-result/awaiting-observation/stale/ambiguous restart classification
- Add pause with an exact resumable phase and child-head reconciliation, plus cancellation that
  commits before propagation and remains cancelling until every active D0-D3 child is terminal or
  an evidence-backed current-revision unreachable/ambiguous classification is retained, without
  allowing any classification or late success to manufacture acceptance
- Add closed causally fenced E0 command/event/state reduction, exact idempotent command resolution,
  complete checkpoint replay, read-only projections, and a one-step runtime driver that orders
  reduction, C0 commit, outbox publication, acknowledgement, and checked observation
- Add canonical schema-v1 E0 command, event, and complete-state families 76-78, strict immutable
  fixtures, namespace `0xE001` durability, tag-12 projection support, and corruption/conflict/crash
  matrices across every commit/publish/acknowledge/result boundary
- Extend A2 with nonempty E0 happy-path, fixer-loop, role drift, stale evidence, bounded exhaustion,
  pause, cancellation, restart, malformed protocol, panic, and teardown scenarios
- Add executable Verus refinements for legal phase order, role separation, candidate freshness,
  evidence invalidation, bounded counters, unique directives, cancellation dominance, terminal
  truth, absence of implicit acceptance, and replay equivalence
- Add the complete E0 design, crate README, production operator guide, architecture/formal/CI
  inventories, generated artifacts, and repository development-state documentation

- Implement the complete production D2 Review Engine as a maintainable H-class `peritus-review`
  orchestration crate, preserving B0/B1/B2 acceptance and approval authority while making review,
  finding, disposition, escalation, and restart truth durable and deterministic (#18)
- Bind each review run to a checked immutable B2 acceptance contract and review-policy snapshot,
  exact seven-component `RevisionTuple`, candidate/tree digests, producer identities and ancestry,
  and a domain-separated digest covering every review-relevant input
- Add independently bounded review limits for cycles, assignments, submissions, findings,
  categories, requirements, source locations, evidence, provenance, dispositions, text/path/opaque
  values, and the complete 16 MiB protocol/state boundary
- Add checked reviewer assignments with stable cycle identity and ordinal, canonical contract
  categories, exact C6 context-plan identity, fresh-context fact, reviewer/provider/model identity,
  producer independence, and no-shared-ancestry evidence
- Add atomic structured review submissions and rich stable findings retaining category, severity,
  blocking status, confidence, requirements, source locations, evidence, reproduction, expected
  behavior, remediation, exact affected revision, normalized digest, all source reviewers/cycles,
  and complete append-only disposition history
- Add provenance-preserving duplicate reconciliation that retains absorbed finding identities,
  sources, evidence, and histories while rejecting self/cyclic/conflicting supersession,
  category/revision mismatch, and any provenance loss
- Add explicit fixer responses for fixed, disputed, proposed-supersession, and waiver-requested
  outcomes; keep each finding open until current independent reviewer confirmation or an exact
  externally authorized B1/B2 waiver observation is durably recorded
- Enforce finding conservation across reviewer-confirmed resolution, invalidation, supersession,
  and externally authorized waiver, with no implicit closure through fixer claims, malformed
  input, cancellation, exhaustion, missing evidence, or historical state
- Compute required review count, category coverage, distinct reviewer, producer independence,
  distinct C6 context, distinct model family, distinct provider, no-shared-ancestry, and fresh
  context as separately named quorum dimensions rather than a lossy composite result
- Add exact revision advance semantics that retain all historical review/finding/waiver evidence
  while excluding every stale cycle, disposition, and authority observation from current quorum,
  conservation, projections, and completion
- Add deterministic finding-set repetition, severity stagnation/regression, disagreement,
  maximum-cycle, and budget-exhaustion accounting with truthful `NeedsHuman`/`Failed` outcomes and
  a closed `Completed`, `NeedsHuman`, `Failed`, or `Cancelled` terminal vocabulary
- Add a causally fenced closed D2 command/event/state reducer covering genesis, revision advance,
  assignment, submission, reconciliation, fixer response, reviewer confirmations, waiver request
  and observation, cycle/run cancellation, budget exhaustion, failure, and finalization
- Add canonical schema-v1 D2 codecs for inert B3 families 53 review-command, 54 review-event, and
  55 review-state, strict tag/bounds/trailing-byte rejection, deterministic digests, immutable
  fixtures, and a rebuildable non-authoritative D2 projection
- Add C0 `Review` aggregate tag 9 and namespace `0xD201` atomic event/checkpoint composition with
  aggregate/state compare-and-swap, exact command idempotency, conflict detection, genesis semantic
  replay, and complete checkpoint equivalence validation
- Extend C0 to schema version four with a backup-required, exact-source-digest table-copy migration
  that widens only aggregate-kind constraints from tags 1–8 to 1–9, validates row counts/metadata,
  preserves historical rows and frames byte-for-byte, and supports exact version-three restore
- Add B2 `ReviewObservation`, `FindingObservation`, and previously authorized `WaiverObservation`
  projection without giving D2 any waiver-issuance, provider/tool execution, workspace mutation,
  or overall run-acceptance path
- Add executable Verus refinements and ordinary-Rust witnesses for bounds, reducer fences, exact
  freshness, independent quorum, disposition legality, finding conservation, truthful terminal
  state, oscillation limits, replay equivalence, and the absence of implicit success
- Extend A2 with ten runtime-neutral D2 scenarios covering lifecycle, quorum, independence,
  reconciliation, stale revision, resolution, waiver, restart, oscillation, and malformed
  submission, including fail-closed negative oracles
- Add real SQLite restart/idempotency/conflict/corruption and schema-migration coverage, domain and
  adversarial codec matrices, generated protocol/schema/client metadata, architecture and strict
  no-cheating command inventories, the grounded D2 design, crate README, and production operator
  guide

- Implement complete production D1 Gate Engine and C7 Trace/Telemetry (#17)
- Implement the complete production D1 Gate Engine boundary with a maintainable H-class
  `peritus-gates` orchestration crate and the required narrow `peritus-tools-quality` extensions,
  without introducing another process, shell, sandbox, workspace, or acceptance-authority path
  (#17)
- Bind every gate run to one validated immutable B2 acceptance contract, exact seven-component
  `RevisionTuple`, deterministic proven gate order, complete set of explicit quality definitions,
  and physically distinct clean read-only C1 snapshot before an effect can be requested
- Add canonical gate descriptors and plans whose domain-separated digests cover every execution-
  and interpretation-relevant check field, dependency, evidence requirement, retry bound,
  environment, resource profile, parser, deadline, snapshot, and revision binding
- Add a closed causally fenced D1 command/event/state machine for start, prepare, dispatch,
  observation, reconciliation, retry, cancellation, evidence publication, and finalization, with
  deterministic dependency blocking and canonical aggregation independent of result arrival order
- Persist attempt intent before dispatch and terminal truth before dependency or acceptance
  advancement, resolving uncertain C0 appends by the original command identity and request digest
  and refusing to redispatch an effect whose post-crash outcome remains indeterminate
- Treat only a newly committed dispatch transition as permit-bearing; an exact already-resolved
  retry is idempotent without recreating a permit, while a later durable checkpoint requires replay
  instead of installing stale local state or executing a stale effect
- Distinguish success, candidate failure, infrastructure failure, cancellation, timeout, malformed
  output, incomplete evidence, exhaustion, blocking, and indeterminate recovery as closed typed
  outcomes; only complete fresh success evidence can satisfy a required gate
- Enforce nonzero per-gate attempt limits, fresh action identities, reconciliation-before-retry,
  idempotent cancellation, no dispatch after cancellation begins, and durable terminal/recovery
  classification for every active attempt before a run may terminate
- Extend `peritus-tools-quality` with deterministic acceptance bindings, a strict closed decoder for
  its structured `quality.run` result, JSON-success evaluation, complete artifact/result checks, and
  construction that admits only the exact clean immutable snapshot selected by D1
- Add normalized D1 evidence requests binding gate/run/execution/attempt/result identities, exact
  revision and clean-snapshot provenance, complete finalized artifact references, and the committed
  C0 event; incomplete or mismatched evidence is permanently non-passing
- Bind every evidence receipt to a canonical domain-separated publication covering the committed
  result position and digest, revision, snapshot, ordered requirements, and exact artifact
  identity/digest/completeness/provenance, including gates whose requirement set is empty
- Bind every replayed or started engine to its originating C0 store identity, reject foreign-store
  commits and evidence publication before mutation or publisher invocation, verify the result
  record against that authoritative journal, and require one-to-one evidence discharge by rejecting
  repeated evidence identities, record digests, or journal provenance across distinct requirements
- Add canonical schema-v1 D1 codecs for inert B3 families 50–52, permanent `Gate` aggregate tag 7,
  atomic event/checkpoint journal composition, genesis replay, checkpoint equivalence checks, and a
  rebuildable non-authoritative gate projection
- Add executable Verus refinements and ordinary-Rust witnesses for dependency readiness, exact
  freshness, bounded attempts, terminal pass truth, replay equivalence, deterministic aggregation,
  and the absence of implicit success
- Add D1 reducer, planning, codec, replay, durability, clean-snapshot, quality-adapter, cancellation,
  retry, parser-corruption, artifact-publication, and inspect/edit/run/test integration coverage,
  plus the grounded design, crate README, and production operator guide

- Implement the complete production C7 observation boundary as separate maintainable H-class
  `peritus-trace` and `peritus-telemetry` crates, keeping durable causal facts distinct from derived
  metrics/export state and preventing either crate from granting execution or acceptance authority
  (#17)
- Add canonical nonzero 16-byte trace and 8-byte span identities, one-based span sequencing,
  structural parents, canonical prior-event sets, observed wall/monotonic time, closed observation
  kinds, sorted safe attributes, sorted redaction decisions, and exact cross-subsystem bindings
- Validate causal refinement across session, run, attempt, turn, action, provider, tool, gate, and
  gate-execution identities, including parent latest-event continuity and same-trace predecessor
  existence without treating timestamps or telemetry as authoritative ordering
- Add deterministic family-60/schema-1 trace encoding with permanent `Trace` aggregate tag 8,
  exact-duplicate recognition, changed-duplicate rejection, aggregate/frame/causal validation, and
  byte-identical projection replay from C0 integrity exports
- Add a C0-backed trace store that observes and compares aggregate heads, binds finalized encrypted
  vault artifacts as journal dependencies, appends exact inert frames, resolves uncertain command
  acknowledgements safely, and returns correlation receipts that cannot authorize work
- Add a redaction boundary whose default observation vocabulary contains no arbitrary text or raw
  byte field, zeroizes consumed sensitive payloads, and emits only omission or a digest/size/
  finalization/quarantine/encryption-checked artifact vault reference
- Add closed redaction-safe diagnostics and non-authoritative metric projections for providers,
  tools, gates, budgets, retries, cancellation, recovery, resources, exporter failures, drops, and
  shutdown, with stable metric names and low-cardinality typed dimensions
- Add OpenTelemetry-compatible spans, events, and metric points with exact identity widths, parent,
  timestamps, status, and safe attribute values, plus immutable idempotent export batches and
  acknowledgement identities that reject partial or mismatched success claims
- Add a capacity-checked telemetry queue with deterministic reject-newest or drop-oldest policy,
  checked monotonic accepted/drop/export accounting, stable batch ranges and digests, full retention
  after exporter failure, and removal only after exact acknowledgement
- Add bounded shutdown flushing and durable export checkpoints published through synchronized atomic
  replacement, with restart validation for stream/projection identity, future positions, corruption,
  and deterministic recovery accounting when restored observations exceed buffer capacity
- Define export checkpoint V2 around the highest contiguous final-disposition prefix, proving every
  covered sequence was either exactly acknowledged or deterministically dropped before restart;
  reject legacy V1 markers closed, preserve gaps under both overflow policies, and make identical
  checkpoint retries repeat directory synchronization and retention pruning before reporting success
- Add executable C7 Verus obligations for sequencing, causal facts, redaction decisions, replay
  equivalence, authority preservation, queue bounds, monotonic accounting, and acknowledgement
  legality, together with domain/codec/storage/projection/redaction/buffer/export/recovery tests
- Add seeded canary coverage proving sensitive prompt, model, tool, secret, environment, workspace,
  and artifact content is absent from `Debug`, `Display`, error chains, frames, projections, metrics,
  and export values, plus the grounded C7 design, crate READMEs, and production operator guide

- Extend C0 to schema version 3 with append-only Gate and Trace aggregate identities and an exact-
  source-digest, backup-required v2-to-v3 migration that rebuilds constrained journal tables,
  preserves existing rows byte-for-byte, count-checks replacements, and validates new appends
- Register inert B3 families 50 gate-command, 51 gate-event, 52 gate-state, and 60
  trace-observation; regenerate the reviewed JSON Schema and TypeScript protocol artifacts without
  moving D1/C7 typed DTO ownership into the foundation layer
- Extend A2 with ten runtime-neutral D1 gate cases, nine C7 trace/telemetry cases, and negative
  implicit-success/default-surface-leakage oracles; the complete conformance target now runs 42
  deterministic fresh-subject cases
- Register all three new crates in the workspace, architecture ownership/layer/class policy,
  strict no-cheating Verus verify/build closure, local Just recipes, reproducibility fixtures,
  Linux hosted verification, and fresh-main formal-governance workflow without weakening any
  existing Ubuntu, macOS, Windows, dependency, lint, documentation, or proof gate
- Update the root development state, C0 migration/durability guidance, A2 catalog documentation,
  formal-foundation command inventory, D1/C7 operating guides, and next-boundary roadmap so D2 is
  identified as the next functional slice after this paired delivery

- Implement the complete production D0 Agent Loop boundary with a maintainable H-class
  `peritus-agent` orchestration crate, small pure-domain/runtime modules, a cooperative one-action
  driver surface, and explicit composition of the completed B0/B1/B3 and C0-C6 contracts (#16)
- Add the durable inner-turn lifecycle from context preparation through model streaming,
  independently authorized tool proposals/execution/result recording, iterative context rebuild,
  and non-accepting completion proposals, including explicit pause, resume, cancellation,
  provider/tool failure, legal retry, malformed response, interruption, limit exhaustion, and
  crash-recovery paths
- Add causally fenced deterministic D0 commands/events/state with checked logical revision,
  aggregate sequence, predecessor event and prior/successor state digests, exact immutable turn
  binding, typed stable rejection/recovery classes, and replay equivalence tests
- Add canonical inert B3 agent command, event, and state families 40-42 with complete bounded
  counters, revision bindings, opaque payload digests, adversarial codec/fixture coverage, and
  redacted Debug surfaces that never disclose provider, model, or tool content
- Extend C0 with the permanent `Agent` aggregate tag 6, schema-version-two fresh databases, a
  backup-required v1-to-v2 migration that rebuilds constrained tables byte-for-byte, restart-safe
  state checkpoints, and a rebuildable agent projection over exact journal observations
- Add atomic D0 journal composition that cross-checks command/event/checkpoint bindings and commits
  the event plus replacement state under aggregate-head and state-revision compare-and-swap, along
  with checked restart loading that refuses missing, stale, or mismatched checkpoints
- Add role-scoped context preparation that retrieves C6 memory before selection, materializes it as
  explicitly delimited non-authoritative evidence with retained source provenance, executes
  dependency-complete C6 token selection, and maps every typed render segment into a separately
  delimited provider-neutral C5 message without authority promotion
- Make C6 compaction operational by installing a validated derived node, removing only admitted
  replaceable sources, rewriting and deduplicating live dependent edges, retaining exact audit
  lineage separately, and rejecting graph drift, protected/required sources, cycles, missing
  dependencies, or a result that does not reduce selected tokens
- Add a versioned canonical codec for all normalized C5 `EventEnvelope` variants, including exact
  identity/order/digest metadata and rejection of unknown versions/tags, truncation, trailing data,
  malformed nested values, or values outside provider-protocol limits
- Add a pull-based C5 model session that keeps exactly one normalized envelope pending until its D0
  journal event is committed, then advances the response reducer, preserving durable stream order,
  duplicate handling, fragmented output/tool assembly, terminal truth, usage high-water accounting,
  cancellation, and explicit EOF failure
- Add a profile-bound persisted-continuation restore seam to provider core with default unsupported
  behavior and exact OpenAI background-response restoration only when immutable profile revision,
  advertised resumability, response identity, and cursor semantics agree
- Persist each bounded canonical provider envelope in its D0 event, rebuild the complete C5 reducer
  prefix before exact continuation restore, require the restored response identity and cursor to
  match, and continue from the next cursor without replaying acknowledged semantics
- Add C5-to-C4 tool planning that converts only completely reduced model calls into bounded inert
  C4 envelopes, validates current exposure and schemas before authority, rejects duplicate actions,
  and gives model output no dispatcher or effect permit
- Add bounded tool coordination through the sole C4 router, requiring the complete independently
  committed authorization request for every dispatch, serializing mutations, permitting bounded
  parallel inspection/execution only when descriptors allow it, and retaining original proposal
  order independently from physical completion order
- Add cooperative long-running tool polling, bounded stdin/PTY/signal control, cancellation and
  recovery through C4-owned handles, explicit success/failure/cancel/timeout/indeterminate terminal
  observations, and post-crash active-call classification that never redispatches an uncertain
  effect
- Add checked D0 structural accounting for provider events, output bytes, tool calls/results,
  context cycles, concurrent calls, and transitions plus a concrete B1 reservation lifecycle for
  model/tool effects: checked plans, held-to-active activation, C5 usage high-water observations,
  exact terminal token/cost/time reconciliation, attempt/retry charging, and conservative
  indeterminate settlement, with no wrapping or placeholder-success path
- Add structured completion proposals bound to exact workspace/specification revisions, fresh
  evidence references, context/model/tool transcript digests, unresolved uncertainties, and a
  requested next phase; D0 completion explicitly does not accept, waive, promote, or mark gates
  successful
- Extend A2 with a nonempty D0 conformance catalog covering complete inspect/edit/run/test,
  pause/resume, cancellation, provider reduction and retry safety, tool authorization/control,
  bounded parallel result ordering, budget exhaustion, completion eligibility, prefix replay, and
  crash recovery without uncertain-effect redispatch
- Add the complete D0 grounded design, crate README, production operating guide, formal obligations,
  fake provider/tool integration matrices, architecture registration, generated protocol clients,
  and updated repository development-state documentation

- Implement the complete production C6 Context and Memory boundary with separate maintainable
  `peritus-role`, `peritus-context`, and `peritus-memory` orchestration crates (#15)
- Project every canonical B1 actor role into an explicit non-widening context policy, including
  writer, reviewer, fixer, evaluator, and evolver profiles plus restricted service/worker/plugin
  profiles, without introducing another security-role identity or issuing capabilities
- Add checked ordered capability views whose Verus specification proves every visible operation
  remains permitted by the exact B1 actor role, along with presentation, contribution, freshness,
  memory, hidden-reasoning, and producer-ancestry controls
- Require an independent reviewer view to use fresh read-only context, exclude producer-hidden
  reasoning and memory-derived producer rationale, and preserve every B2 reviewer-independence
  requirement as evidence that later orchestration must establish
- Add bounded provenance-aware context nodes that bind content digests, authority and trust
  ceilings, semantic classes, required/optional mode, priority, recency, role visibility, and
  canonical dependencies, with graph rejection for duplicates, missing edges, and cycles
- Add deterministic required-first context selection with complete dependency closures, atomic
  optional admission, stable integer precedence, explicit selection/omission reasons, checked node
  and byte limits, and exact context-window, output-reserve, protocol-overhead, used, and remaining
  token accounting
- Add transactional compaction validation over selected canonical source ranges, including policy
  binding, digest and lineage checks, visibility, range ordering, token savings, protected policy,
  specification, user-instruction, capability, and blocking-finding classes, and trust-preserving
  derivation only when every source and policy allow it
- Add provider-neutral render plans whose individually delimited segments preserve source identity,
  message role, provenance, authority, trust, context class, content digest, and bounded bytes
  without concatenating untrusted text into an elevated instruction channel
- Add immutable scoped memory records with stable identities and revisions, original provenance,
  source events, supporting and contradicting evidence, bounded confidence and relevance features,
  logical observations, review/expiry state, feedback, and canonical content digests
- Add explicit memory review, quarantine, release, expiry, supersession, forgetting, and tombstone
  transitions; tombstones bind prior digest and revision and deterministically dominate replayed
  records at or below the deleted revision
- Add deterministic filter-before-rank retrieval with exact project/workspace/repository/actor/role
  scope checks, lifecycle and tombstone exclusion, confidence and feature policy, bounded integer
  score components, stable identity tie-breaking, result/token limits, and an explanation for every
  selected or excluded record
- Add rebuildable canonical memory indexes and digests over active records and tombstones, with
  deterministic posting lists and equivalence tests that keep storage an implementation detail for
  the future C0/D0 composition boundary
- Add context and memory poisoning matrices proving instruction-like repository, external, tool,
  provider, and recalled text remains quoted non-authoritative evidence with its original
  provenance and cannot become policy, a capability, or an authority transition
- Add focused no-cheating Verus roots for role narrowing, context graph/selection/accounting and
  compaction invariants, memory non-authority, lifecycle advancement, tombstone dominance, and
  bounded retrieval; register all three crates in architecture, ordinary-API, reproducibility, and
  hosted formal-governance command surfaces
- Add the complete C6 design, operating guide, crate READMEs, construction/selection/compaction/
  rendering/lifecycle/index/retrieval test matrices, and the documented D0 integration boundary

- Implement the complete production C5 Model Providers boundary with six maintainable model-layer
  crates for the provider-neutral protocol, shared provider core, OpenAI, Anthropic, Google, and
  explicitly configured compatible endpoints (#14)
- Add protocol v1 checked identities, messages and bounded multimodal content, JSON Schema tools
  and results, strict structured output, reasoning summaries and opaque replay state, persistence
  and continuation controls, deterministic canonical request identity, complete capability/profile
  negotiation, and immutable accepted/rejected compatibility fixtures
- Add ordered normalized response streams for text, reasoning, tool arguments, refusals, usage,
  cache, rate limits, response identity, provider extensions, finish reasons, and typed terminal
  failures, with exact duplicate handling, fragmented UTF-8/JSON assembly, bounded reduction, and
  fail-closed malformed/incomplete/cancelled outcomes
- Add a hardened provider-core effect boundary with validated redacted endpoints and credentials,
  Reqwest/Rustls ownership, bounded HTTP and byte streams, SSE/NDJSON framing, cancellation-aware
  backoff, conservative retry and ambiguous-submission classification, owned stream teardown, and
  an explicit server-side response cancellation seam, plus bounded subprocess invocation with
  explicit arguments/environment isolation, output/deadline ceilings, cancellation, and child reap
- Add the current first-party OpenAI Responses adapter with multimodal/tool/structured-output and
  reasoning projection, prompt caching, usage/rate metadata, heterogeneous SSE normalization,
  background exact-cursor continuation, and confirmed background response cancellation
- Add the current first-party Anthropic Messages adapter with top-level system projection,
  multimodal content and tools, structured output, adaptive thinking and opaque signature replay,
  prompt caching, required version/beta headers, cumulative usage, and Messages SSE normalization
- Add separately profiled account-backed OpenAI Codex and Anthropic Claude transports that use the
  providers' already-authenticated official executables as stateless credential-owning routers,
  disable native tools and ambient integration surfaces, normalize schema-constrained text/inert
  tool proposals/usage, never inspect account tokens, and advertise advisory output limits
- Add both documented stable-v1 Google Gemini families: Interactions for new development and
  generateContent/streamGenerateContent for existing integrations, including tools, multimodal
  content, response schemas, thinking signatures, cached content, safety/finish observations,
  retention/state policy, and explicit `x-goog-api-key` authentication without an SDK `v1beta`
  fallback
- Add separately validated compatible Responses and Chat Completions profiles whose explicit
  dialect, paths, authentication, framing, supported fields, mappings, lifecycle, retry guarantees,
  limits, and response-ID semantics default to the minimum safe feature set rather than inferred
  OpenAI parity
- Extend A2 with a nonempty fourteen-case provider suite and owned deterministic loopback servers,
  including bounded multi-exchange scripts on one stable endpoint, covering capability honesty,
  ordering/deduplication, fragmented tools, malformed/incomplete streams, cancellation,
  authentication, rate limiting and retry-after, transient recovery, ambiguous submission, usage,
  redaction, and selected/foreign adapter isolation
- Qualify both account-backed routes with fresh-subject hermetic fake executables covering exact
  invocation isolation, structured output, terminal failure, cancellation, and child reap without
  a provider installation, account credential, or live network; separately qualify shared process
  output-overflow and timeout handling through portable real-process tests
- Add provider-specific immutable request/stream/error fixture corpora with manifests and SHA-256
  inventories, crate READMEs, the C5 operating guide, and hosted Linux/macOS/Windows provider
  qualification wiring
- Add thirteen Verus-verified C5 functional-core obligations and connect ordinary runtime paths to
  checked capability intersection, reducer transition and terminal facts, exact deduplication,
  completed-fragment predicates, monotonic usage, retry legality, and provider non-authority

- Implement complete production C3 Platform Security Backends (#12)
- Implement complete production C2 Process/Sandbox Backplane (#11)
- Implement C1 Git, workspace, and atomic patching (#10)
- Implement C0 journal, projections, artifacts, migrations, and evidence (#9)
- Implement B3 domain protocol and canonical codec (#8)
- Implement B0 lifecycle kernel (#7)
- Implement B2 acceptance specification and quality policy (#6)
- Implement B1 policy, leases, budgets, and approvals (#5)
- Implement A2 test/conformance foundation (#4)

### Fixed
- Honor the checked managed-network connection budget for upstream socket reads and writes instead
  of imposing an undocumented 100 ms cutoff, with a delayed redirect-response regression test
- Canonicalize account-runtime fake executable working directories so their isolation assertions
  remain valid across macOS `/var` and `/private/var` path aliases
- Make explicit fake-HTTP release points wait briefly for an already-issued peer close instead of
  racing the macOS loopback stack with a single immediate observation
- Make malformed completed UTF-8 or JSON establish an irreversible failed reducer terminal, reject
  all post-terminal events without replacing the original outcome, and classify explicit
  non-accepting HTTP responses separately from ambiguous post-send failures
- Preserve bounded first-party provider request IDs on normalized success and failure observations
  while continuing to exclude credentials, response bodies, prompts, outputs, and tool arguments
  from diagnostics and fake-server artifacts
- Exercise retry-after and transient recovery through real two-exchange HTTP servers for every
  direct HTTP adapter instead of substituting an in-memory transport for those conformance paths
- Restore hosted Linux, macOS, and Windows runner portability across native sandbox, process, Git,
  patch, network, durable registry, and tool-shell test boundaries (#12)
- Remove macOS socket-close races from the managed-proxy worker-backpressure conformance test
- Stabilize hosted Windows native shell conformance polling under runner scheduling delays
- Make managed-proxy HTTP fixtures issue each complete request in one socket write, and preserve the
  process cleanup regression's timing distinction with realistic hosted-runner scheduling allowance

### Changed
- Implement complete production D2 Review Engine (#18)
- Implement C4 tool system (#13)
- Document production architecture for Verus-first coding harness (#1)
- Implement A1 formal foundation (#3)
- Implement A0 workspace and toolchain foundation (#2)
