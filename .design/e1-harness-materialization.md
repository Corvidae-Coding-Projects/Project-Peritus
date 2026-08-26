# Feature: E1 Production Harness Materialization

## Summary

E1 adds `crates/orchestration/peritus-harness` as the Verus-first, durable source of truth for
Project Peritus harness definitions. A harness is no longer an informal directory or a mutable
prompt bundle. It is a checked graph of typed components, an immutable content-addressed revision,
and an exact materialization plan applied only through the existing C1 workspace boundary. C0
retains revision history, pending materializations, receipts, artifact dependencies, and complete
restart state. B3 owns the inert command/event/state frames, and A2 exercises the public contract
independently of implementation details.

The component catalog covers instructions, roles, tools, middleware, skills, collaboration,
memory, gates, orchestration, provider profiles, observability, and evolution definitions. It also
models security-root policy, human authority, sealed evaluators, trust-boundary specifications,
and production-promotion rules as protected controlled assets. Compatibility, graph structure,
declared authority, file inventory, provenance, and executable digests are validated before a
revision can exist. A successor cannot rewrite history or mutate protected assets.

This design is frozen before implementation agents begin. It deliberately implements the complete
E1 boundary while leaving E2 failure diagnosis, E3 empirical evaluation, F0 campaign/promotion
authority, and G0 daemon/CLI presentation to their already-defined slices.

## User-visible behavior

1. A repository commits `.peritus-harness/manifest.toml` and every declared component beneath
   `.peritus-harness/components/`. Loading occurs through `ReadOnlyWorkspace`; symlinks, special
   files, missing declarations, duplicate targets, undeclared component files, malformed TOML,
   and digest mismatches fail with precise diagnostics.
2. E1 parses the inert manifest into typed component declarations. Every declaration has a stable
   identifier, closed kind, schema version, source and target path, exact content digest and size,
   owner, provenance, dependency requirements, compatibility contract, declared authority, and
   optional executable artifact digest.
3. A graph check resolves all references, rejects cycles and incompatible dependency versions or
   kinds, validates the authority ceiling of each component and dependency closure, and returns a
   deterministic topological order.
4. `HarnessRevision::genesis` creates a content-addressed revision only from the complete checked
   graph and exact component bytes. `HarnessRevision::successor` links an immutable predecessor,
   increments the logical revision, preserves the lineage, and rejects any protected-asset drift.
5. Registering a revision stores its exact canonical description and artifact dependencies in a
   C0 harness aggregate. A repeated identical command is idempotent; an identity or digest
   conflict is rejected without changing the aggregate.
6. A caller requests materialization into an exact C1 workspace snapshot. E1 first commits a
   deterministic plan and durable outbox directive, then reads verified component artifacts,
   constructs one bounded `PatchSet`, applies it through `WorkspaceGateway`, and asks C1 to create
   the immutable successor candidate.
7. Materialization changes only paths owned by the prior or target harness receipt. Existing
   unrelated files are never deleted. A stale preimage, dirty workspace, authorization failure,
   artifact mismatch, or Git failure remains a typed non-success outcome.
8. A crash before the plan commit has no effect. A crash after the plan commit replays the same
   pending directive. A crash after C1 mutation or candidate creation is reconciled against the
   exact patch, snapshot, tree, and manifest identities before E1 records success.
9. Rollback materializes an immutable ancestor with an explicit rollback reason. It creates a new
   workspace snapshot and receipt; it does not edit the old revision, erase descendants, move a
   production pointer, or silently promote anything.
10. Projections can be rebuilt from genesis and expose revision history, graph summaries, pending
    work, materialization receipts/failures, and artifact roots without providing mutation,
    authorization, acceptance, evaluation, or promotion authority.

## Requirements

### Manifest and complete component catalog

- **E1-R001:** The only committed harness entry point is
  `.peritus-harness/manifest.toml`. Schema v1 is strict UTF-8 TOML with unknown fields rejected.
  The loader uses C1 no-follow reads and directory listings; production E1 code does not read or
  write repository paths through `std::fs`.
- **E1-R002:** The closed `ComponentKind` catalog shall include base instruction fragment, system
  instruction fragment, role definition, role prompt, tool descriptor, tool schema, tool
  implementation, tool exposure policy, middleware, context transform, skill bundle, reference
  bundle, sub-agent definition, collaboration definition, memory schema, memory selector, memory
  ranking policy, memory retention policy, memory injection policy, gate definition, gate parser,
  orchestration policy, termination policy, provider capability, provider profile, observability
  policy, redaction policy, analysis policy, evolution strategy, and metric definition.
- **E1-R003:** The controlled `ProtectionClass` catalog shall include evolvable, security root,
  human authority, sealed evaluator, trust boundary, and production promotion. Protection is
  derived from the closed component kind and compiled policy; a manifest cannot downgrade it.
- **E1-R004:** Every component declaration binds `ComponentId`, kind, schema version, source path,
  materialization target, media type, byte length, SHA-256 content digest, owner, provenance,
  ordered dependency requirements, compatibility contract, declared authority set, protection
  class, and optional executable artifact digest. Empty IDs, owners, provenance, or invalid paths
  reject before graph construction.
- **E1-R005:** `ComponentId` is stable within a lineage and independent of list position. Source
  paths must be descendants of `.peritus-harness/components/`; target paths are C1-relative,
  normalized, non-protected workspace paths. Neither path may alias the manifest, `.git`,
  `.peritus`, `.peritus-harness`, or another declaration.
- **E1-R006:** The loader checks exact declared byte count and SHA-256 for every source. Component
  bytes are opaque and may be binary; only the TOML manifest must be UTF-8. The optional executable
  digest is independently bound and cannot substitute for the source content digest.
- **E1-R007:** Recursive C1 inventory of `.peritus-harness/components/` must equal the declared
  source-file set. Missing, duplicate, symlinked, special, oversized, and undeclared entries reject.
  Empty directories are not components and do not affect revision identity.
- **E1-R008:** `HarnessLimits` bounds manifest bytes, component count, dependency edges, dependency
  fan-out, per-component bytes, total materialized bytes, revision history, receipt history, event
  bytes, state bytes, and retained diagnostics. Manifest limits may tighten compiled ceilings but
  may never widen them.

### Compatibility, graph, and declared authority

- **E1-R010:** `DependencyRequirement` binds a component ID, required kind, inclusive compatible
  schema interval, and optional exact content digest. The interval must be nonempty. An exact
  digest strengthens rather than replaces kind and version checks.
- **E1-R011:** `CompatibilityContract` binds the component's own supported schema interval and
  explicit provider/platform feature requirements. Schema intervals and sorted feature sets are
  canonical; unknown feature tags remain inert and cannot be treated as supported.
- **E1-R012:** Graph checking rejects duplicate IDs, missing dependencies, self-edges, cycles,
  incompatible kinds or versions, exact-digest disagreement, noncanonical feature sets, and
  unsatisfied provider/platform requirements. It returns a deterministic topological order with
  `ComponentId` as the tie breaker.
- **E1-R013:** Declared authority uses a closed set covering context read, workspace read,
  workspace mutation, process execution, network access, secret reference, approval request,
  acceptance observation, evaluation input, and promotion proposal. These bits describe intended
  exposure only and never mint a B1 capability or authorize an effect.
- **E1-R014:** Each component kind has a compiled maximum authority set. A declaration exceeding
  that ceiling rejects. The transitive authority required through dependencies must also fit the
  depender's ceiling; dependency composition cannot smuggle a forbidden authority into a role,
  prompt, skill, parser, metric, or other inert component.
- **E1-R015:** Protected controlled assets cannot be dependencies of an incompatible evolvable
  component where that dependency would delegate security-root, human, evaluator, trust-boundary,
  or promotion authority. E1 validates the declaration but continues to rely on B1/C4/C6 for
  actual runtime authority.
- **E1-R016:** `CheckedHarnessGraph` is constructible only by the complete graph validator. It
  retains declarations, resolved edges, topological order, graph digest, aggregate authority,
  feature requirements, protected-asset inventory, and exact component artifact roots.

### Immutable content-addressed revisions and history

- **E1-R020:** `HarnessRevision::genesis` accepts only a checked graph and complete verified
  component contents. Its digest is a domain-separated SHA-256 over canonical schema version,
  lineage seed, manifest digest, graph digest, ordered complete declarations, component content
  digests/sizes, and executable artifact digests.
- **E1-R021:** `HarnessId` is deterministically derived from the genesis revision digest and remains
  stable for the lineage. `HarnessRevisionNumber` begins at one. Every successor has the same
  harness ID, the exact predecessor digest, and predecessor number plus one; full revision digest,
  not the logical number, distinguishes branches.
- **E1-R022:** A revision is immutable after construction. The public API exposes observations and
  canonical bytes but no field mutation, unchecked deserialization, caller-selected digest, or
  mutable graph access.
- **E1-R023:** `HarnessRevision::successor` compares the complete protected-asset inventory against
  its predecessor. Adding, removing, renaming, reordering, changing contents, schema, owner,
  provenance, dependency, compatibility, authority, path, or executable digest of a protected
  asset rejects.
- **E1-R024:** Evolvable components may change only by creating a successor. Component removal and
  replacement must leave a valid complete graph and exact target inventory. Historical revisions
  and their canonical bytes remain queryable and retain their artifact roots.
- **E1-R025:** `HarnessHistory` is an append-only bounded DAG keyed by full digest. It accepts one
  genesis and checked direct successors, rejects orphan/duplicate/conflicting revisions, exposes
  ancestry and branch queries, and produces deterministic canonical snapshots.
- **E1-R026:** A rollback target must be an existing ancestor of the selected source revision.
  Rollback is represented in the materialization reason and receipt; history is not rewound or
  deleted and no revision is relabeled as current production.
- **E1-R027:** The ordinary agent run APIs receive a `RevisionTuple` containing the exact harness
  ID and revision. They cannot change that tuple. E1 construction and materialization are distinct
  administrative operations and do not add an ordinary-run harness mutation route.

### Exact C1 materialization

- **E1-R030:** `MaterializationPlan` binds plan ID/digest, command/event identity, harness and full
  revision digest, graph digest, target workspace ID/generation/revision/snapshot/tree, reason,
  prior receipt when present, ordered file operations, expected preimages, output digests/sizes,
  and total limits.
- **E1-R031:** Plan construction is pure and deterministic. It emits create/replace operations for
  all target components and delete operations only for paths proven owned by the exact prior E1
  receipt and absent from the target revision. Duplicate targets, ancestor collisions, protected
  workspace paths, and operations outside the owned set reject.
- **E1-R032:** Materialization reads every payload from a finalized active C0 artifact with an exact
  bounded read, verifies metadata size and SHA-256 again, and constructs one C1 `PatchSet` bound to
  the target workspace identity and expected preimages. E1 performs no raw filesystem mutation.
- **E1-R033:** The C1 flow is `WorkspaceGateway::apply_patch` followed by
  `WorkspaceGateway::create_candidate` under separately validated target-owned authorizations.
  Success requires the exact patch identity, successor snapshot, Git commit/tree, workspace
  manifest artifact, and clean installed revision returned by C1.
- **E1-R034:** A `MaterializationReceipt` binds the plan, source revision, prior receipt, applied
  patch, authorization action identities/digests, before/after workspace identities, candidate
  commit/tree, C1 manifest artifact, exact output file inventory, timestamps/causal event identity,
  and an overall canonical digest.
- **E1-R035:** The runtime never reports success from a planned patch, successful write alone,
  guessed directory state, or unverified candidate. A C1 dirty/indeterminate outcome remains
  pending reconciliation or records a typed failure without fabricating a receipt.
- **E1-R036:** Re-materializing the identical revision into the identical resulting snapshot is
  idempotent and returns the retained receipt. A different payload under the same plan or command
  identity is a conflict. A newer workspace head requires a new checked plan.
- **E1-R037:** Rollback uses the same materializer and safety checks as forward materialization.
  Only the target revision differs; no privileged deletion, direct Git operation, or protected
  component mutation path exists.

### Durable aggregate, replay, and projection

- **E1-R040:** The closed harness command vocabulary shall cover register genesis, register
  successor, plan materialization, acknowledge directive delivery, record materialization,
  record materialization failure, reconcile pending materialization, and retire a settled receipt
  from the bounded hot projection. No command promotes or evaluates a harness.
- **E1-R041:** Accepted commands emit a corresponding semantic event and exact successor state,
  binding `CommandId`, `EventId`, harness ID, expected sequence/predecessor, prior state digest,
  command kind/digest, revision digest, artifact roots, and materialization correlation where
  relevant. Rejection returns unchanged state and no event.
- **E1-R042:** Planning commits a `MaterializationPlanned` event, complete checkpoint, artifact
  dependencies, and one stable outbox directive atomically before C1 is called. Recording success
  or failure consumes only the exact matching pending plan and atomically settles its directive.
- **E1-R043:** Schema-v1 B3 command/event/state frames use protocol families 79, 80, and 81.
  Decoded frames are inert until checked by E1 constructors/reducer. Unknown tags, noncanonical
  order, duplicate keys, illegal lengths, invalid UTF-8 fields, and trailing bytes reject.
- **E1-R044:** C0 aggregate tag 13 is `Harness` and checkpoint namespace `0xE101` owns the complete
  harness aggregate state. Migration v6 widens the v5 aggregate constraint from tags 1-12 to 1-13
  without altering prior tags, rows, event bytes, checkpoints, outbox data, or artifact references.
- **E1-R045:** One accepted transition atomically appends its family-80 event, installs the complete
  family-81 checkpoint, records finalized artifact dependencies, resolves exact command
  idempotency, and applies any outbox mutation under C0 head/state CAS.
- **E1-R046:** Recovery replays from genesis, verifies sequence/predecessor/state/event digests and
  exact checkpoint equality, then reconciles pending materialization against C1 observations. It
  may redeliver the same idempotent directive, record an exact already-completed candidate, or
  quarantine a conflict; it cannot guess an ambiguous write into success.
- **E1-R047:** Replay rejects orphan successors, protected drift, revision-digest disagreement,
  missing artifact roots, impossible materialization ordering, mismatched receipts, duplicate
  pending work, stale acknowledgements, state-size overflow, and terminal/checkpoint divergence.
- **E1-R048:** `HarnessProjection` is rebuildable from events and exposes lineage/revisions/branches,
  component/graph summaries, protected inventory, pending plan/delivery state, receipts/failures,
  rollback ancestry, and artifact roots. Projection corruption is repaired by replay and provides
  no mutation or promotion authority.
- **E1-R049:** B3 generated Rust/TypeScript schemas, complete binary fixtures and SHA-256 manifests,
  C0 migration fixtures/backups/restores, A2 positive and negative conformance cases,
  architecture/formal inventories, crate documentation, repository README, and CHANGELOG are part
  of the slice rather than follow-up work.

### Verus and maintainability

- **E1-R050:** Component uniqueness, dependency resolution, graph acyclicity, topological
  completeness, compatibility, authority non-widening, protected-asset invariance, digest binding,
  append-only history, ancestor-only rollback, owned-path confinement, state transitions,
  idempotency, and replay equivalence shall have executable Verus specifications/proofs wherever
  the pinned toolchain supports the concrete representation.
- **E1-R051:** The crate contains no `assume`, `admit`, axiom, trusted body, `unsafe`, hidden public
  precondition, placeholder, ignored test, state-machine macro, or convenience authority bypass.
  Verus wrappers and ordinary Rust APIs are both tested.
- **E1-R052:** Public fields remain private and constructors return typed errors. `lib.rs` remains
  below 80 lines. Production modules target 400 lines and never exceed 700; domain, manifest,
  materialization, aggregate, wire, durability, replay, runtime, and projection concerns remain
  separate.
- **E1-R053:** E1 depends on public B1/B3/C0/C1 contracts and shared foundation types. It does not
  depend on provider executables, model SDKs, raw shell/process adapters, UI/daemon code, E2/E3/F0,
  or concrete filesystem mutation outside C1.

## Acceptance criteria

| Criterion | Required evidence |
|---|---|
| Complete catalog | round-trip and rejection cases for every component and protection class |
| Manifest fidelity | exact C1 inventory, digest/size/path checks, undeclared/missing/symlink matrix |
| Graph correctness | duplicate/missing/cycle/version/kind/digest/feature/authority matrices |
| Immutable revisions | deterministic genesis/successor, protected drift matrix, branch/ancestry queries |
| Exact materialization | create/replace/owned-delete, stale preimage, unrelated-file preservation, C1 candidate |
| Rollback | ancestor-only prior revision materializes as a new workspace snapshot and receipt |
| Crash recovery | failpoint after plan commit, publish, patch, candidate, receipt commit, and restart |
| Durable truth | families 79-81, tag 13, namespace `0xE101`, checkpoint/replay/idempotency conflicts |
| Migration | v5 fixture -> v6 preserves all rows/bytes; backup/restore and rollback guidance verified |
| Projection | rebuild from genesis equals live projection and exposes no mutation/promotion route |
| Conformance | nonempty A2 manifest/graph/revision/materialization/replay/panic/teardown catalogs |
| Formal quality | strict no-cheating Verus, API audit, Clippy, rustdoc, targeted tests, serialized Gate A |

## Current architecture

B1 owns actual authority and capability checks; E1 authority declarations are descriptive ceilings.
B3 owns inert schema families and generated fixtures. C0 provides the SQLite journal, complete
checkpoints, command idempotency, outbox, artifact dependencies, and finalized artifact store. C1
provides no-follow workspace reads and the only authorized mutation/candidate boundary. Existing
D0-D3/E0 domains carry an immutable `RevisionTuple` but deliberately do not define or mutate its
harness contents.

No current crate defines the complete harness component catalog, validates a harness dependency
graph, constructs content-addressed immutable revisions, retains revision history, or turns a
revision into an exact C1 patch and candidate. The architecture registry already reserves
`peritus-harness` as a V/H orchestration crate depending on B2/B3/C0/C1.

## Proposed design

### Checked domain pipeline

```text
ReadOnlyWorkspace inventory + strict manifest
  -> LoadedHarness (exact bytes and declarations)
  -> CheckedHarnessGraph (resolved compatibility and authority)
  -> HarnessRevision genesis/successor (content addressed, immutable)
  -> HarnessHistory registration (append-only durable DAG)
  -> MaterializationPlan (exact target snapshot and owned paths)
  -> C0 planned directive -> C1 patch/candidate -> C0 receipt
```

Parsing, canonicalization, graph validation, revision construction, history transitions, planning,
and reducers are pure. Artifact reads, C0 commits/outbox, and C1 calls sit behind narrow runtime
ports. This keeps proofs and property tests independent of SQLite, Git, and the host filesystem.

### Canonical identity

All identities use domain-separated, length-prefixed canonical bytes with sorted maps/sets and
closed numeric tags. Revision identity commits the complete graph rather than only concatenated
file bytes, so a path, owner, compatibility, authority, provenance, dependency, or protection
change necessarily creates a different digest. The full SHA-256 revision digest is authoritative;
logical revision numbers are display/order metadata and may repeat across branches.

### Materialization and restart protocol

The aggregate commits a pending plan and outbox record before the runtime performs C1 work. The
runtime verifies active artifacts, builds the exact `PatchSet`, and obtains separate B1/C1 permits
for patch and candidate creation. On restart, the driver compares the pending plan with the exact
workspace generation/revision, C1 patch identity, Git candidate identity, and workspace manifest
artifact. Matching completion is recorded; an untouched target is retried idempotently; partial or
conflicting state is quarantined for explicit reconciliation.

Old target paths may be deleted only when the exact prior receipt proves that E1 owned them. This
makes both forward replacement and ancestor rollback useful in real repositories without turning
the harness materializer into a general-purpose file deletion API.

### Module layout and frozen ownership

```text
crates/orchestration/peritus-harness/
  Cargo.toml                                  # implementation lane 2
  README.md                                   # implementation lane 2
  src/
    lib.rs                                    # implementation lane 2
    domain/
      mod.rs                                  # implementation lane 1
      identity.rs                             # implementation lane 1
      component.rs                            # implementation lane 1
      authority.rs                            # implementation lane 1
      compatibility.rs                        # implementation lane 1
      graph.rs                                # implementation lane 1
      revision.rs                             # implementation lane 1
      history.rs                              # implementation lane 1
      limits.rs                               # implementation lane 1
      error.rs                                # implementation lane 1
      verified.rs                             # implementation lane 1
    manifest/{mod,document,loader,inventory}.rs # implementation lane 2
    materialization/{mod,plan,receipt,executor}.rs # implementation lane 2
    aggregate/{mod,command,event,state,reducer}.rs # implementation lane 2
    wire/{mod,command,event,state,canonical}.rs # implementation lane 2
    durability/{mod,binding,commit,recovery}.rs # implementation lane 2
    runtime/{mod,driver,ports}.rs              # implementation lane 2
    replay.rs                                  # implementation lane 2
    projection.rs                              # implementation lane 2
  tests/domain_*.rs                            # implementation lane 1
  tests/{manifest,materialization,durability,replay,fixtures}_*.rs # lane 2
```

The root integrator exclusively owns workspace `Cargo.toml`/`Cargo.lock`, `architecture.toml`, this
design, B3 registry/generated output/fixtures, C0 aggregate enum/artifact-store read API/migration
and migration fixtures, A2 conformance, shared formal inventories, repository README/CHANGELOG and
operational docs, integration review, Git, PR, and hosted verification. The two implementation
lanes must not edit one another's listed files or any root-owned file. The design commit is frozen
before either lane begins.

### Alternatives considered

A filesystem-only manifest would be easy to load but could not provide immutable revision history,
artifact retention, restart-safe materialization, or exact replay. It is rejected.

A mutable `current_harness` table would make rollback look simple but would rewrite identity and
mix materialization with later F0 promotion authority. Immutable revisions plus append-only
receipts preserve truth and allow F0 to make a separate evidence-backed promotion decision.

Letting E1 write files or invoke Git directly would duplicate C1 and bypass its authorization,
preimage, dirty-state, and candidate semantics. E1 instead produces an exact plan and delegates
the mutation to C1.

Combining all component, graph, persistence, runtime, and projection logic into one module would
make proof boundaries and parallel maintenance brittle. The frozen module layout keeps pure domain
logic, effects, codecs, and replay independently testable without creating tiny generic utility
modules.

## Data and compatibility

Families 79-81, aggregate tag 13, namespace `0xE101`, component/protection/authority tags,
canonical field order, digest domains, and revision predecessor semantics become immutable on
merge. Unknown tags and trailing bytes reject. Migration v6 preserves all v5 data and widens only
the aggregate-kind constraint. A committed `.peritus-harness/manifest.toml` declares schema v1;
future schema versions require explicit decoders and migration rather than permissive parsing.

The artifact store gains a narrow bounded verified read API suitable for materialization. It
requires active finalized metadata, enforces the caller's maximum byte count, uses no-follow
content access, verifies size and SHA-256, and returns owned bytes. Existing publication and GC
semantics remain unchanged.

## Failure handling

- Manifest, graph, revision, plan, wire, durability, workspace, artifact, and reconciliation
  failures use distinct typed codes and recovery classes.
- Invalid input and rejected commands emit no event and retain the exact prior state.
- A missing/corrupt/quarantined artifact prevents planning or execution and retains diagnostic
  identity without exposing partial bytes as valid content.
- C1 stale generation/revision/preimage, dirty worktree, rejected authorization, Git failure, and
  manifest-finalization failure are never converted into materialization success.
- A pending directive survives restart with the same identity and bounded deliveries. Exact prior
  completion is reconciled; conflicting state is quarantined.
- Bounds end work with explicit failures rather than truncating components, history, diagnostics,
  or receipts silently.

## Security considerations

E1 descriptions do not grant authority. B1 remains authoritative for every artifact, workspace,
and action permit; C1 remains authoritative for mutation and Git candidate creation. Protected
asset classification is compiled and successor invariance is checked structurally. The loader is
confined to C1 no-follow reads, and the executor only receives verified artifact bytes and an exact
owned-path patch plan. No manifest text, prompt, tool descriptor, or decoded wire frame executes by
being loaded.

The implementation concentrates on realistic failures for this application: malformed or stale
repository contents, dependency drift, accidental authority widening, protected-asset changes,
artifact corruption, stale workspaces, process crashes, retries, and ambiguous partial C1 state.

## Verification

Targeted domain/property/proof, manifest, graph, revision, history, materialization, wire,
durability, replay, migration, C0/B3, A2 conformance, Clippy, and rustdoc checks run serially with
`CARGO_BUILD_JOBS=1`. The pinned Verus toolchain runs strict no-cheating checks for E1 proof files.
After targeted checks, exactly one complete local `just gate-a` runs with no overlapping heavy
command. Hosted Gate A/Foundation matrices must be green on Linux, macOS, and Windows. One hosted
rerun is permitted only for a clearly transient runner failure.

## Rollout and rollback

E1 lands through signed commits and a protected PR. Schema migration v6 requires a verified v5
backup and fixture before widening tag 13. Before tag-13 data exists, code rollback can restore the
v5 binary/schema. After harness events exist, downgrade requires restoring the captured v5 backup
or a later forward repair; an old binary must not open v6 data. Harness rollback itself is the
normal ancestor materialization operation and does not require database rollback.

Completion requires the signed merge on `main`, `main == origin/main`, a fresh-main serialized
local Gate A, green hosted Gate A/Foundation, issue #20 closed with delivery evidence, and no
remaining E1 locks.

## Open questions

None. Component/protection catalogs, limits and authority semantics, identities, revision DAG,
protected invariance, C1 materialization order, rollback behavior, protocol families, aggregate
tag, checkpoint namespace, migration, ownership, verification, and slice boundaries are fixed.

## Out of scope

- E2 failure diagnosis, causal analysis, blame assignment, and harness-health scoring.
- E3 statistical evaluation, baselines, experiments, effect sizing, and regression detection.
- F0 variant campaigns, evaluator sealing mechanics, promotion decisions, production activation,
  and automatic harness evolution.
- G0 CLI/TUI/daemon APIs, packaging, installer behavior, and remote fleet coordination.
- New capability, acceptance, approval, waiver, evaluator, or production-promotion authority.
- Provider-specific model prompts, SDK behavior, or executable adapters beyond referring to their
  immutable component and executable artifact digests.
