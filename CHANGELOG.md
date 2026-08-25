# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
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
- Implement C4 tool system (#13)
- Document production architecture for Verus-first coding harness (#1)
- Implement A1 formal foundation (#3)
- Implement A0 workspace and toolchain foundation (#2)
