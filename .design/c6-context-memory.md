# C6 Context and Memory Design

## Summary

C6 supplies the complete context-construction and derived-memory boundary needed by the durable
agent loop. It adds three orchestration-layer crates:

- `peritus-role`, a verified projection from B1 security roles into context visibility,
  contribution, freshness, and presentation policy;
- `peritus-context`, a verified provenance graph, deterministic selector, token-budget planner,
  compaction validator, and provider-neutral render-plan builder; and
- `peritus-memory`, a verified scoped-memory lifecycle, evidence/confidence model, deterministic
  retrieval planner, quarantine/forgetting behavior, tombstones, and rebuildable index model.

These crates are control-plane libraries. They do not invoke models, execute tools, mutate
workspaces, persist records, or issue capabilities. D0 will combine their immutable plans with C4
tools, C5 provider profiles, and C0 durable records.

The preferred architecture keeps the three crates separate. `peritus-role` is frozen first.
`peritus-context` and `peritus-memory` then build independently against that contract, with their
integration expressed through immutable, provenance-preserving candidate data. This is preferred
over a single context/memory crate because selection, compaction, retention, and retrieval evolve
at different rates and need independent tests and ownership.

The design verdict is **ready for implementation**. The architecture, authority boundary,
dependency direction, failure behavior, and acceptance evidence are fully specified below.

## User-visible behavior

C6 has no direct CLI surface, but it defines behavior that future CLI, daemon, and agent-loop
surfaces must expose consistently:

1. Every piece of model-visible context has explicit provenance, authority class, trust class,
   digest, token estimate, recency, role visibility, and dependency edges.
2. Selection is deterministic for identical inputs. Required content is either included with all
   required dependencies or rejected with a typed reason; it is never silently omitted.
3. Application/system policy and immutable specification material outrank derived memory, tool
   output, repository content, and external material. Lower-authority text remains inert content
   even when it contains instruction-like language.
4. Context planning honors an explicit input-token budget and reserves output/headroom tokens.
   Budget failure identifies required items responsible for the failure.
5. Compaction is a validated derivation, not an in-place rewrite. A compacted node cites its
   source ranges and policy revision; protected policy/specification nodes cannot be summarized.
6. A render plan preserves boundaries and provenance. It contains ordered, typed segments rather
   than a single concatenated prompt and remains provider-neutral.
7. Memory records are scoped, evidence-backed derived claims. Retrieval is deterministic and
   explainable through component scores and exclusion reasons.
8. Expired, quarantined, forgotten, unsupported, or out-of-scope memories are not injected.
9. Forgetting emits a tombstone that suppresses the record during replay and index rebuild.
10. A reviewer receives a fresh, read-only context view that excludes producer-hidden reasoning.
    A writer, fixer, evaluator, and evolution agent receive different context views without
    changing the B1 role's operation permissions.

## Requirements

### Role policy

- **C6-R001:** `peritus-role` shall use `peritus_policy::ActorRole` as the canonical role identity
  and shall not define a competing security-role enum.
- **C6-R002:** Each supported harness role shall have a deterministic context policy covering
  visible provenance classes, allowed contribution classes, required context classes, freshness,
  hidden-reasoning visibility, memory visibility, and presentation profile.
- **C6-R003:** Capability views shall contain only operation classes permitted by
  `ActorRole::permits_operation`; the crate shall not issue, consume, widen, or persist B1
  capabilities.
- **C6-R004:** Reviewer policy shall require fresh context, prohibit producer-hidden reasoning and
  memory-derived producer rationale, and expose B2 independence requirements without weakening
  them.
- **C6-R005:** Writer and fixer roles shall not receive acceptance, waiver, policy-amendment, or
  harness-promotion capability views. Reviewer and evaluator roles shall not receive workspace
  mutation capability views.
- **C6-R006:** Unsupported B1 service/worker/plugin roles shall receive explicit restricted
  profiles rather than being silently mapped to a privileged harness role.

### Context graph and selection

- **C6-R010:** Every `ContextNode` shall have a stable node identifier, content digest,
  provenance, authority, trust, content kind, token estimate, recency sequence, requirement mode,
  role-visibility set, and canonically ordered dependency identifiers.
- **C6-R011:** Constructors shall reject zero token estimates, duplicate dependencies,
  self-dependencies, noncanonical dependency/visibility order, and inconsistent
  provenance/authority/trust combinations.
- **C6-R012:** The graph constructor shall reject duplicate node identifiers, missing
  dependencies, and dependency cycles.
- **C6-R013:** Selection shall be deterministic and stable. Required closure is selected first;
  optional nodes are ranked by authority, requirement, explicit priority, recency, and stable
  identifier tie-breaks.
- **C6-R014:** A selected node shall imply selection of its complete dependency closure.
- **C6-R015:** Nodes hidden from the selected role shall never appear in selection or rendering.
- **C6-R016:** A required node that is hidden, missing a visible dependency, or cannot fit shall
  produce a typed planning error. Selection shall not return a partial success.
- **C6-R017:** Token arithmetic shall be checked. The plan shall separately report context-window
  capacity, reserved output, reserved protocol overhead, usable input, used input, and remaining
  input.
- **C6-R018:** Optional groups that cannot fit with their full dependency closure shall be omitted
  atomically with an explainable reason.

### Compaction and rendering

- **C6-R020:** A compaction proposal shall name a new derived node, the compaction-policy digest,
  and a canonically ordered list of nonempty source ranges.
- **C6-R021:** Compaction validation shall reject missing sources, range errors, overlapping or
  noncanonical ranges, digest mismatches, source cycles, hidden sources, and output estimates that
  are not smaller than the replaced selected material.
- **C6-R022:** System policy, application policy, immutable specifications, active user
  instructions, capability facts, and unresolved blocking findings shall be protected from
  summarization.
- **C6-R023:** A validated compacted node shall retain `DerivedCompaction` provenance, untrusted
  trust unless every input is trusted and policy permits trust preservation, and dependency links
  to every source node.
- **C6-R024:** Rendering shall preserve segment role, provenance, authority, trust, digest, and
  content kind. It shall never promote repository, external, memory, tool, agent, or review text
  to system/application authority.
- **C6-R025:** Render ordering shall be deterministic and respect precedence while keeping each
  node as a separate segment. Provider-specific message encoding belongs to D0/C5 integration.

### Memory lifecycle and retrieval

- **C6-R030:** A `MemoryRecord` shall contain a stable identifier, scope, source event set, claim
  type, content digest, confidence, supporting evidence, contradicting evidence, creation/review
  observations, optional expiry, retrieval features, lifecycle state, and revision.
- **C6-R031:** Memory constructors shall reject empty source sets, empty scopes, invalid bounded
  scores, noncanonical evidence/features, evidence present in both supporting and contradicting
  sets, expiry before creation, and zero revisions.
- **C6-R032:** Memory shall always be derived, non-authoritative context. No memory API shall
  return a B1 capability, authority transition, acceptance decision, waiver, specification
  amendment, or harness promotion.
- **C6-R033:** Lifecycle transitions shall be explicit and checked: active memories may be
  reviewed, quarantined, expired, superseded, or forgotten; forgotten memories are terminal and
  produce a tombstone; quarantine release requires a later review observation and revision.
- **C6-R034:** A tombstone shall bind memory identifier, last known revision, deletion observation,
  reason, and prior digest. Replay shall make deletion win over records at or below that revision.
- **C6-R035:** Retrieval shall filter by exact scope compatibility, role policy, lifecycle,
  quarantine, expiry, minimum confidence, required features, and tombstones before ranking.
- **C6-R036:** Ranking shall be deterministic using bounded integer components for scope
  specificity, relevance, confidence, evidence balance, recency, and feedback. Stable identifier
  order shall break equal scores.
- **C6-R037:** Retrieval shall return an explanation for every candidate: selected with component
  scores, or excluded with a typed reason. Selected total estimated tokens shall not exceed the
  query budget.
- **C6-R038:** Negative feedback, contradiction, and stale review shall reduce ranking or trigger
  quarantine under explicit policy; they shall not be discarded silently.
- **C6-R039:** Indexes shall be derived from canonical records and tombstones. Rebuild from the
  same ordered input shall produce the same active index and digest.
- **C6-R040:** Memory text originating in repositories, tools, providers, or external sources
  shall retain that provenance when materialized as context and shall be delimited as quoted
  evidence, never executable instructions.

### Engineering and verification

- **C6-R050:** All deterministic validation, ranking, selection, lifecycle, and budget logic that
  Verus can express shall live in Verus-verified code with executable postconditions or proof
  obligations.
- **C6-R051:** Public structs shall keep fields private and expose checked constructors and
  read-only accessors. Errors shall be typed, stable, actionable, and comparable in tests.
- **C6-R052:** Production source shall contain no `unsafe`, reachable placeholder success path,
  `todo!`, ambient I/O, global mutable state, wall-clock lookup, provider call, or effect handle.
- **C6-R053:** Each crate shall keep the root module below 80 lines, ordinary source files below
  the 400-line soft limit where practical and below the 700-line hard limit without exception.
- **C6-R054:** The crates shall pass formatting, build, all-target/all-feature tests, strict
  Clippy, rustdoc warnings, architecture, ordinary-API audit, focused no-cheating Verus
  verification, and verified release build.

## Acceptance criteria

| Criterion | Required evidence |
|---|---|
| Role policies cannot widen B1 roles | exhaustive role/operation matrix tests and Verus proof that every projected operation is B1-permitted |
| Review context is fresh and independent | reviewer profile tests plus B2 independence projection tests |
| Graphs are valid and selections deterministic | construction failure matrix, permutation/property corpus, exact golden plans |
| Required context is never silently lost | hidden/missing/cyclic/over-budget tests proving typed failure |
| Token plans are bounded | boundary and overflow tests plus verified arithmetic postconditions |
| Compaction preserves provenance and protected content | full proposal rejection matrix, successful lineage fixture, poisoning corpus |
| Rendering preserves authority boundaries | exact render-segment fixtures and precedence tests |
| Memory cannot grant authority | public API audit, compile/runtime assertions, `INV-021` proof obligation |
| Lifecycle and deletion are replay-safe | transition table tests, tombstone dominance tests, rebuild equivalence fixtures |
| Retrieval is bounded, deterministic, and explainable | ranking permutation tests, score fixtures, budget/filter exclusion tests |
| Poisoned memory stays inert | repository/external/tool instruction-like payload corpus retaining quoted provenance |
| Crates are maintainable | architecture/source-layout/ordinary-API gates, docs, no forbidden placeholders |

No criterion may be waived by returning an empty plan, ignoring a test, replacing a
production-path dependency with an in-memory test double, or relying on undocumented ordering.

## Current architecture

B1 already owns the stable `ActorRole`, nonconfigurable operation separation, capability scopes,
and authorization transitions. B2 owns immutable acceptance/review contracts and
`ReviewerIndependence`. B3 owns durable domain protocol and bounded codecs. C0 owns durable event,
artifact, and projection storage. C4 owns tool authorization. C5 owns provider-neutral request and
event protocols.

C6 sits in the reserved `crates/orchestration` layer. The layer may depend on foundation, state,
runtime, tools, model, and orchestration, but this slice deliberately keeps its production
dependencies to A1/B1/B2 contracts. It does not need C5: render plans are neutral and D0 will map
them into `peritus-model-protocol::Message` values. It does not need C0: C6 defines canonical
records and replay/rebuild calculations while a future composition boundary persists events.

The repository has no existing orchestration crate and therefore no compatibility migration is
required. The canonical architecture registry already assigns these exact crate names to C6.

Reference implementations informed, but do not constrain, this design:

- Codex CLI demonstrates separate context fragments, bounded compaction lifecycle, and child role
  configuration that can reduce but not replace parent authority.
- LemonHarness demonstrates explicit workspace state, execution records, context budgets, memory,
  and implementer/reviewer phases.
- NexAU-AHE demonstrates configurable compaction triggers/strategies and session memory, while C6
  replaces its string-oriented and in-memory behavior with typed provenance and verified plans.

## Proposed design

### Crate dependency graph

```text
peritus-types ───────────────┐
                            ├── peritus-role ───────┐
peritus-policy ──────────────┤                       ├── peritus-context
peritus-spec ────────────────┘                       └── peritus-memory

peritus-context and peritus-memory do not depend on each other.
D0 converts retrieved memory candidates into context nodes through public checked constructors.
```

`peritus-role` uses verification class `V`. `peritus-context` and `peritus-memory` use class `H`
because their ordinary-safe boundaries compute canonical SHA-256 content and index digests through
the existing H-class codec; their deterministic planning, validation, lifecycle, and ranking cores
remain Verus-verified and all three packages set `package.metadata.verus.verify = true`. Future I/O
or persistence adapters still belong in separate composition crates rather than these pure
contracts.

### `peritus-role`

Module layout:

```text
src/
  lib.rs
  capability_view.rs
  context_policy.rs
  context_class.rs
  harness_role.rs
  independence.rs
  presentation.rs
  verified.rs
tests/
  role_matrix.rs
  reviewer_independence.rs
```

`HarnessRole` covers `Writer`, `Reviewer`, `Fixer`, `Evaluator`, and `Evolver` and has an exact
mapping to a B1 `ActorRole`. Restricted profiles for other B1 roles are produced through
`RoleProfile::for_actor_role`, so no role is implicitly privileged.

`ContextClass` is a presentation/selection classification, not provenance. It includes immutable
policy, acceptance specification, active user request, repository instructions/source/diff,
workspace state, gate evidence, tool observation, memory evidence, prior findings/resolutions,
agent progress, and hidden reasoning. Context nodes retain the more fundamental provenance and
authority types owned by `peritus-context`.

`ContextPolicy` uses canonical `ContextClassSet` values and explicit booleans/enums for fresh
context, memory use, producer ancestry, and presentation. `CapabilityView` is an ordered subset of
`OperationClass`; construction proves each operation is permitted by the underlying B1 role.
It never contains a `Capability` or `CapabilityScope`.

`ReviewIndependenceView` copies the immutable required facts from B2
`ReviewerIndependence` together with a required fresh-context flag. It is evidence requested from
D2, not a claim that independence has already been established.

### `peritus-context`

Module layout:

```text
src/
  lib.rs
  authority.rs
  budget.rs
  compaction.rs
  compaction/range.rs
  content.rs
  error.rs
  graph.rs
  graph/validation.rs
  identity.rs
  node.rs
  plan.rs
  precedence.rs
  provenance.rs
  render.rs
  selection.rs
  selection/closure.rs
  selection/ranking.rs
  trust.rs
  verified.rs
tests/
  compaction_matrix.rs
  graph_matrix.rs
  poisoning.rs
  selection_matrix.rs
  fixtures.rs
fixtures/v1/
  MANIFEST
  SHA256SUMS
  *.plan
```

Identity wrappers are fixed-size byte values so they are deterministic and cheap to verify:
`ContextNodeId([u8; 16])`, `CompactionPolicyId(Sha256Digest)`, and `ContextPlanId(Sha256Digest)`.
They own no random generator; callers inject identities.

`Provenance` contains `System`, `Application`, `User`, `Repository`, `External`, `Memory`, `Tool`,
`Agent`, `Review`, and `DerivedCompaction`. `AuthorityClass` is ordered separately and contains
`SystemPolicy`, `ApplicationPolicy`, `AcceptanceSpecification`, `UserInstruction`, and
`NonAuthoritative`. A compatibility function rejects combinations such as external provenance
claiming application authority. `TrustClass` contains `Trusted`, `Constrained`, and `Untrusted`;
provenance establishes the maximum trust that a constructor may accept.

`ContextNode` stores metadata and typed content bytes. Content is bounded by an explicit
`ContextLimits` policy. The digest is verified against content by the constructor; text is not
reparsed for authority. `RequirementMode` is `Required`, `DependencyRequired`, or `Optional`.
`ContextGraph::new` accepts nodes in canonical ID order and validates identity, edges, and DAG
shape.

`TokenBudget` is constructed from context window, reserved output, and reserved overhead with
checked subtraction. `SelectionPolicy` carries role policy, allowed node/byte/token limits, and
optional ranking weights. The selector:

1. filters nodes by the frozen role visibility contract;
2. forms complete dependency closures for required nodes;
3. rejects unsatisfied required closure or required budget overflow;
4. ranks optional roots with an integer tuple, never floating point;
5. admits an optional root only with its entire not-yet-selected closure; and
6. emits ordered selected entries and explicit omission records.

`CompactionProposal` contains source ranges over selected nodes. `validate_compaction` produces a
`ValidatedCompaction` only after all protected-content, lineage, range, digest, visibility, and
budget checks pass. The function does not generate prose or claim summary fidelity; a provider may
propose content, but Peritus validates whether the derivation is admissible and retains the source
lineage.

`RenderPlan` contains ordered `RenderSegment` values. Each segment contains its source identity,
context class, model-facing message role, provenance, authority, trust, digest, and bounded content.
No rendering method returns a capability or provider transport. Provider-specific conversion is a
D0 adapter with an exhaustive role map.

### `peritus-memory`

Module layout:

```text
src/
  lib.rs
  claim.rs
  confidence.rs
  error.rs
  evidence.rs
  feedback.rs
  identity.rs
  index.rs
  lifecycle.rs
  lifecycle/transition.rs
  record.rs
  retrieval.rs
  retrieval/filter.rs
  retrieval/ranking.rs
  scope.rs
  tombstone.rs
  verified.rs
tests/
  index_rebuild.rs
  lifecycle_matrix.rs
  poisoning.rs
  retrieval_matrix.rs
  fixtures.rs
fixtures/v1/
  MANIFEST
  SHA256SUMS
  *.index
```

The crate uses caller-supplied `MemoryId([u8; 16])`, C0 `EventId`/`EvidenceId` foundation types,
and `Sha256Digest`. `Observation` is an explicit logical epoch/tick pair; no wall clock is read.
`MemoryScope` contains optional project/workspace/repository/actor/role dimensions plus a required
scope kind. At least one durable scope dimension is required, and query compatibility is exact or
explicitly broader according to `ScopePolicy`.

`Confidence` and retrieval components are bounded integer basis points (`0..=10_000`) rather than
floating point. `EvidenceSet` stores canonical unique IDs. `ClaimType` distinguishes fact,
preference, procedure, outcome, warning, constraint, and hypothesis without conferring authority.
`RetrievalFeatures` are canonical key/digest/weight triples; they do not embed a provider-specific
vector index.

`MemoryRecord` is immutable. Lifecycle methods return a revised record or tombstone and require a
monotonically increasing revision and observation. `MemoryState` contains `Active`, `Quarantined`,
`Expired`, and `Superseded`; `Forgotten` is represented only by a tombstone so deleted content is
not retained in the active model.

`RetrievalPolicy` defines token/result limits, minimum confidence, accepted claim types, required
review freshness, ranking weights, and quarantine behavior. `RetrievalQuery` contains exact scope,
role profile, observation, query features, and a caller-supplied token budget. Filtering precedes
ranking. The result includes selected `MemoryCandidate` metadata and an `ExcludedMemory` for every
unselected input. Candidate materialization exposes provenance of the underlying source and an
explicit `quoted_evidence` flag. D0 copies this data into a checked context node.

`MemoryIndex::rebuild` consumes canonically sorted records and tombstones, applies tombstone
dominance, excludes inactive records, and constructs deterministic scope/claim/feature posting
lists plus an index digest. The index is an optimization; retrieval against its canonical active
record view is defined to match a full scan.

### Parallel implementation ownership

After the role contract is committed in the working branch:

- context track owns only `crates/orchestration/peritus-context/**`;
- memory track owns only `crates/orchestration/peritus-memory/**`; and
- integration owner retains root `Cargo.toml`, `Cargo.lock`, `architecture.toml`, verification
  manifests, `peritus-role`, shared documentation, conformance wiring, and final fixtures.

The two tracks may read but not edit one another's crate. They communicate through the frozen role
API and the field-level candidate contract in this design, avoiding shared-file collisions.

### Credible alternative and rejection

A credible alternative is a single `peritus-context-memory` crate with one graph containing live
context nodes, compacted nodes, and memory records. This reduces initial type conversion and might
make global token selection shorter.

It is rejected because it couples ephemeral prompt assembly to durable retention policy, makes
forgetting/index rebuild affect the context selector, encourages one large stateful service, and
prevents independent ownership. It would also tempt memory entries to inherit authority directly
from context nodes. The separate-crate design preserves one-way immutable data flow and keeps
authority compatibility checks at both boundaries.

## Data and compatibility

C6 introduces no persisted wire format and therefore requires no C0 migration. Fixture encodings
are test-only canonical textual records with versioned manifests and SHA-256 inventories. Public
enum order is not treated as a wire tag unless explicitly documented.

All identifiers and digests are caller-supplied fixed-width values. All ordered collections require
canonical ascending order and uniqueness. All numeric scores use bounded integers. These choices
make equality, replay, cross-platform behavior, and future B3 serialization deterministic.

Future persistence shall use new B3 commands/events and C0 projections. Compatibility rules will
be additive: unknown enum tags fail closed, new optional metadata receives explicit defaults, and
tombstones remain valid across index schema generations. C6 types shall not derive a general
deserializer that bypasses checked constructors.

## Failure handling

Each crate defines a stable error kind and structured error carrying the affected collection,
field, and identifier where appropriate. Expected failures include invalid bounds, canonical-order
violations, incompatible authority/trust, missing graph dependencies, cycles, hidden required
content, budget exhaustion, protected compaction sources, illegal lifecycle transitions, stale
observations/revisions, scope mismatch, and tombstone conflicts.

Planning is transactional in memory: constructors and planners return either a complete valid
value or an error and do not mutate inputs. Retrieval returns a complete explanation including
normal exclusions; malformed canonical state remains an error. Arithmetic uses checked operations
and reports overflow rather than wrapping.

There are no retries, clocks, network operations, file operations, or process operations in these
crates. D0/C0 callers decide persistence and retry policy based on typed failures.

## Security considerations

The principal security property is provenance separation, not content inspection. Instruction-like
text from repositories, external pages, tools, agents, reviews, or memory remains typed data at its
original authority/trust ceiling. Rendering retains boundaries and never changes that ceiling.

Role projections can only narrow B1 permissions. `peritus-role` has no capability issuance API,
and context/memory contain no capability type. Reviewer profiles exclude mutation and producer
hidden reasoning. Memory cannot amend policy/specification, accept work, waive findings, grant
tools, or rewrite harness components.

Bounds limit graph size, dependency fan-out, content bytes, token estimates, evidence counts,
retrieval features, selected results, and score arithmetic. This prevents unbounded allocations at
the control boundary without inventing speculative adversaries. Poisoning tests focus on likely
inputs: repository instructions, copied web text, tool output, model summaries, and stale memories.

Digests provide identity and replay binding, not proof that prose is true. Confidence and evidence
are explicit inputs, not automatic trust escalation. Compaction validation proves lineage and
policy compliance but does not prove semantic faithfulness; reviewers and later evaluation remain
responsible for that evidence.

## Verification

### Verus obligations

The slice registers semantic obligations covering:

- `INV-021 MemoryNonAuthority`;
- role capability views are subsets of B1 `permits_operation`;
- selected nodes are visible and their dependency closure is complete;
- selected and reserved token totals never exceed the declared context window;
- required-node failures cannot produce a plan;
- compaction sources remain linked and protected classes are never compacted;
- memory lifecycle revisions/observations advance monotonically;
- forgotten/tombstoned records are absent after rebuild; and
- retrieval results satisfy lifecycle, scope, role, confidence, expiry, and budget filters.

Every executable pure validator exposes postconditions used by focused proof roots. No proof root
may assume the property it claims or use `external_body`, `admit`, `assume`, `axiom`, or an approved
TCB exception.

### Test matrix

Tests cover ordinary constructor and accessor behavior, exhaustive role matrices, graph cycles and
dependency closure, token boundaries and overflow, deterministic selection across permutations,
compaction rejection reasons, render ordering, memory lifecycle transitions, evidence conflicts,
scope filtering, ranking ties, expiry/quarantine/tombstone dominance, index rebuild equivalence,
and poisoned instruction-like content from every non-authoritative provenance.

Focused commands use system-memory-aware parallelism:

```text
cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 cargo test -p peritus-role -p peritus-context -p peritus-memory \
  --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-role -p peritus-context -p peritus-memory \
  --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=2 RUSTDOCFLAGS='-D warnings' cargo doc \
  -p peritus-role -p peritus-context -p peritus-memory --all-features --no-deps --locked
just source-layout
just architecture
just ordinary-api
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-role --package peritus-context \
  --package peritus-memory --all-features --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
CARGO_BUILD_JOBS=1 cargo verus build --package peritus-role --package peritus-context \
  --package peritus-memory --all-features --release --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

Before merge, full Gate A and Foundation matrices must pass locally where applicable and on hosted
Ubuntu, macOS, and Windows runners. The merged `main` revision receives a fresh final gate.

## Rollout and rollback

Rollout is additive:

1. register the three empty packages and verification obligations;
2. freeze and verify `peritus-role`;
3. implement context and memory in parallel;
4. integrate fixtures, conformance, documentation, and the C6 cross-crate poisoning matrix;
5. merge only after candidate checks are green.

Because no existing runtime consumes C6 and no persisted schema changes, rollback is a normal Git
revert of the C6 merge. No data rollback or compatibility shim is required. Once D0 persists C6
records, later changes must follow B3/C0 migration policy; that is outside this slice.

Operationally these crates allocate only bounded in-process data and perform no I/O. D0 will record
plan/retrieval digests and typed failures in the journal and C7 trace. Performance baselines for
graph selection, retrieval, and rebuild are recorded now; production SLO qualification remains H3
without weakening correctness.

## Open questions

There are no implementation-blocking open questions.

Future slices must decide:

- the B3 wire tags for persisted context/memory commands and events;
- which C5 tokenizer estimator supplies provider-specific estimates to D0;
- the storage/index backend used by C0 projections at production scale;
- the exact model/provider diversity rule used by D2 review quorum; and
- user-facing retention defaults and CLI wording in G1.

Those decisions do not change C6's pure contracts: callers inject estimates, observations,
features, policies, and durable identifiers.

## Out of scope

The following belong to later canonical slices and are not smuggled into C6:

- model invocation, streaming, tokenizer network calls, and provider-specific message encoding
  (C5/D0);
- durable command/event encoding and persistence (B3/C0/D0);
- tool execution or capability issuance/consumption (B1/C4/D0);
- the edit/run/test turn state machine and pause/cancel recovery (D0);
- gate DAG execution and freshness evaluation (D1);
- finding lifecycle, review quorum adjudication, and waiver/acceptance (D2);
- scheduling, collaboration, tracing, telemetry, debugger reports, evaluation campaigns, harness
  mutation, daemon/CLI/TUI, and final performance/release qualification (D3 onward).

These are dependency boundaries, not descoping of the production project. C6 implements its full
production contract and leaves later slices to consume it without placeholder behavior.
