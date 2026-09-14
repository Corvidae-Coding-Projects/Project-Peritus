# GAP-03 versioned queue/recovery correction

Status: implemented design under local qualification. This branch changes source and adds compatibility artifacts; no deployment or journal rewrite has been performed.

## Decision

Add an explicit, aggregate-sticky scheduler semantics version and map it one-to-one to the existing B3 frame-header `schema_version`:

| B3 family | schema 1 | schema 2 |
|---|---|---|
| 70 scheduler command | `LegacyQueueV1` | `StrictRecoveryQueueV2` |
| 71 scheduler event | `LegacyQueueV1` | `StrictRecoveryQueueV2` |
| 72 scheduler state | `LegacyQueueV1` | `StrictRecoveryQueueV2` |

The discriminator is the existing big-endian `u16` at frame bytes `8..10`. The B3 header is 16 bytes; reserved flags remain zero at bytes `10..12`. Do not add a payload discriminator, reuse flags, infer semantics from work phases or timestamps, inspect provenance, or guess from whether a state happens to satisfy the strict bound. Those alternatives either create two sources of truth or assign different semantics to the same historical bytes.

Schema 1 keeps its historical reducer and byte-for-byte encoding. Schema 2 is the default for every newly created scheduler aggregate and enforces strict recovery-aware queue pressure. A scheduler aggregate may never mix the two schemas. Existing journals are preserved and remain writable under schema 1 semantics.

This is smaller than a wholly new payload layout: the command, event, and state payload field order can remain unchanged. The header version supplies the semantic discriminator. State and event payload bytes will still usually differ in schema 2 because state digests are version-separated.

## Confirmed defect and required strict behavior

Current admission counts only `Queued | WaitingDependencies | RetryPending` in `crates/orchestration/peritus-scheduler/src/reducer/apply/admission.rs:10-50`, and `AdmitWork` applies that count in `crates/orchestration/peritus-scheduler/src/reducer/apply.rs:115-165`. Decoded-state validation applies the same visible-queue count in `crates/orchestration/peritus-scheduler/src/state/validation.rs:8-36`.

The two regressions in `crates/orchestration/peritus-scheduler/tests/queue_recovery.rs` show the contradiction. With `queued_work = 1`, retryable A is dispatched, B is admitted into the apparently free slot, and A later returns after either worker loss or explicit retryable failure. Live reduction and replay accept the history, but the checkpoint decoder rejects the resulting two waiting records.

For schema 2 define:

```
W = count(Queued | WaitingDependencies | RetryPending)
R = count((Reserved | Running) and attempts_started < maximum_attempts)
strict_pressure = W + R
```

`R` includes every recovery policy. `FailWork { disposition: Retryable }` permits an eligible `Reserved` or `Running` item to become `RetryPending` without consulting `RecoveryPolicy`; restricting `R` to `RetrySafe` would leave the explicit-failure reproduction intact. The relevant paths are `crates/orchestration/peritus-scheduler/src/state/mutation/reservation_command.rs:178-248`, `crates/orchestration/peritus-scheduler/src/reducer/apply/dispatch.rs:45-134`, and `crates/orchestration/peritus-scheduler/src/work/record.rs:202-241`.

Schema-2 admission accepts a new work item only when `strict_pressure < queued_work`, at the same point and with the same preceding rejection priority as the present visible-queue test. Successful admission makes pressure at most `queued_work`. Schema-2 inert validation requires `strict_pressure <= queued_work`.

`Cancelling` is excluded from `R`: retryable failure rejects it and worker loss terminalizes it. A final-attempt `Reserved` or `Running` item is also excluded because it cannot return to a waiting phase. These exclusions keep the invariant strict without reserving capacity that no scheduler transition can refill.

### Strict invariant proof obligation

Let `P = W + R` and assume a valid schema-2 predecessor has `P <= Q`, where `Q = queued_work`.

- Admission is the only transition that creates retained work. It requires `P < Q` and adds one waiting item, so `P' = P + 1 <= Q`.
- Dispatch changes `Queued` to `Reserved` and increments `attempts_started`. If another attempt remains, it moves one unit from `W` to `R`; on the final attempt it removes one unit from `W` and adds none to `R`.
- Start acknowledgement changes `Reserved` to `Running` and preserves whether another attempt remains.
- `RetryWork` changes `RetryPending` to `Queued`; dependency refresh changes `WaitingDependencies` to `Queued`; both preserve `W`.
- Retryable failure changes an eligible `Reserved | Running` item to `RetryPending`, for every recovery policy, moving one unit from `R` to `W`.
- Retry-safe worker loss makes the same `R`-to-`W` move. Ambiguous/fail loss outcomes, completion, non-retryable failure, exhaustion, abandonment, and terminal dependency refresh only remove pressure.
- Cancellation either removes a waiting unit or changes active work to `Cancelling`, which removes any recoverable-active unit. Cancellation acknowledgement and later loss cannot increase pressure.
- Worker-only changes, scheduler phase changes, and finalization do not increase pressure.

Every command applies a finite collection of the per-record changes above. Repeating dispatch, failure/loss, retry, and admission cycles therefore preserves `P <= Q` by induction; the proof must cover the loops, not only one hand-written scenario.

## Exact legacy bound

Schema 1 must retain old admission decisions while rejecting impossible or malicious decoded states. Define:

```
W = count(Queued | WaitingDependencies | RetryPending)
Awork = count(Reserved | Running | Cancelling)
legacy_occupancy = W + Awork
```

Require:

```
legacy_occupancy <= queued_work + active_reservations
```

The addition must be checked or performed in a representation whose maximum is proved to cover both configured limits. Do not use saturating arithmetic. Existing retained-work validation still independently bounds the complete `work` collection.

This is the reachable finite bound, and is stronger than the earlier suggestion `W <= Q + A`. Let `Q = queued_work` and `A = active_reservations`. Genesis has occupancy zero. A valid predecessor already has `Awork <= A` through reservation/active-work validation. Old admission requires `W < Q`; after adding one waiting record,

```
W' + Awork = W + 1 + Awork <= (Q - 1) + 1 + A = Q + A.
```

Dispatch moves one record from waiting to active; acknowledgement stays active; retryable failure and retry-safe loss move one record from active to waiting; `RetryWork` and dependency readiness stay waiting. Active cancellation changes `Reserved | Running` to `Cancelling`, so counting `Cancelling` preserves occupancy until acknowledgement/loss releases it. Terminal outcomes and cancellation of inactive work lower occupancy. Other transitions leave it unchanged. Worker-loss and cancellation loops are repeated applications of these non-increasing per-record changes. Thus every accepted schema-1 transition, and every repeated recovery cycle, preserves `W + Awork <= Q + A`.

`W <= Q + A` alone is too weak. For `Q = 1`, `A = 1`, a decoded state with `W = 2` and `Awork = 1` passes that test but has occupancy 3 above the reachable bound 2. It must be rejected. Omitting `Cancelling` is also weaker than the exact active-ownership invariant because cancelling work retains its reservation.

The pressure/occupancy scans must count work phases directly, before relying on reservation relations. A malicious state must not evade a queue bound by omitting a reservation or corrupting its owner. Later validation must still prove the exact `Reserved | Running | Cancelling` to reservation relation in `crates/orchestration/peritus-scheduler/src/verified.rs` and enforce `reservations.len() <= active_reservations`. The queue scan does not replace those checks.

## In-memory semantic identity

Add a closed public enum, preferably in `crates/orchestration/peritus-scheduler/src/semantics.rs`:

```
pub enum SchedulerSemantics {
    LegacyQueueV1,
    StrictRecoveryQueueV2,
}
```

It needs exact `schema_version() -> u16` and checked `from_schema_version(u16)` mappings. Unknown values are rejected. Add the value to:

- `SchedulerBinding` in `crates/orchestration/peritus-scheduler/src/binding.rs`;
- `SchedulerCommand` in `crates/orchestration/peritus-scheduler/src/command.rs`;
- `SchedulerEvent` in `crates/orchestration/peritus-scheduler/src/event.rs`.

`SchedulerState` obtains semantics from its immutable binding rather than storing a second field. Add projections/accessors and update verified clone/equality/spec relations. `SchedulerProjection`/`ProjectedScheduler` in `crates/orchestration/peritus-scheduler/src/projection.rs:121-183` must expose semantics so a client continuing an old aggregate can choose schema 1 explicitly.

`start` requires command semantics to equal the embedded binding semantics. `decide` treats semantic mismatch as a fence failure after the existing terminal and command-history checks; add the equality to `fences_match` in `crates/orchestration/peritus-scheduler/src/reducer/fences.rs:202-255` without changing terminal/history priority. Reducer-created events copy command semantics. `command_from_event` copies event semantics. Replay requires every event to share the genesis semantic version and rejects a mixed sequence before accepting its domain result.

`crates/orchestration/peritus-scheduler/src/durability/binding.rs:7-33` must explicitly compare command, event, and successor-state semantics in addition to its existing identifiers, revisions, cursors, digests, and payload correspondence.

## Constructor behavior

Keep `SchedulerBinding::new(...)` source-compatible and make it create `StrictRecoveryQueueV2`. The decoder uses a crate-private `from_wire(semantics, ...)`. Do not expose a normal constructor for a new schema-1 aggregate.

Keep `SchedulerCommand::new(...)` as the ordinary schema-2 constructor. It rejects a `StartScheduler` whose binding is not schema 2. Add one state-derived continuation path:

1. `SchedulerCommand::from_state(&SchedulerState, command_id, event_id, kind)` fills the semantic version and all sequence/predecessor/digest/revision fences from authoritative state and rejects another `StartScheduler`.

Keep `from_wire(semantics, ...)` crate-private for versioned decoding and deterministic replay. Production state-derived callers should use `from_state`; in particular `crates/app/peritus-daemon/src/authority/owner/orchestrator/children/scheduler.rs:19-73` currently rebuilds the state and then calls the versionless constructor for pause/resume. The reconstructed command must inherit that state's semantics or a legacy aggregate becomes unwritable.

All current direct `SchedulerBinding::new` tests and qualification fixtures become schema-2 by default. This is deliberate. Tests for historical bytes must decode fixed schema-1 fixtures rather than create new legacy state through the public constructor.

Repository evidence shows no production or test consumer of `SchedulerProjection` beyond its definition and re-export (`rg "SchedulerProjection|ProjectedScheduler" --glob '*.rs'` finds only `src/projection.rs` and `src/lib.rs`). Therefore this branch should not add a speculative projection-based raw command constructor. `ProjectedScheduler` should still expose semantics as truthful aggregate identity, while in-repository writers use `from_state`. Pre-existing external schema-1 clients remain wire-compatible because the server continues accepting valid raw schema-1 non-genesis commands; preserving that wire contract does not require a new source API for creating legacy aggregates.

### Live genesis policy boundary

Constructor visibility is not the security or compatibility boundary. A client can submit hand-built canonical bytes, and the dual decoder must accept a schema-1 `StartScheduler` value so historical genesis events can replay. The live policy is:

```
existing exact command resolution  -> preserve its recorded result
aggregate absent + schema 2 start  -> permit new durable genesis
aggregate absent + schema 1 start  -> reject as unsupported live genesis
aggregate present + matching schema -> permit a non-genesis continuation
aggregate present + other schema    -> reject mixed-version append
```

Apply it in two places, with D3 durability as the authoritative guard:

1. In `crates/app/peritus-daemon/src/domain/dispatch.rs:133-168`, keep both scheduler schemas decodable. After `SchedulerSession::state` has loaded/rebuilt the aggregate, if the result is `None` and the command semantics are not `StrictRecoveryQueueV2`, return `DomainOutcome::Rejected(AppErrorCode::UnsupportedSchema)` before calling `start`. This produces a bounded client rejection. Do not reject schema 1 in `CommandSubmissionFrames::parse` at `crates/app/peritus-app-protocol/src/command/binding.rs:92-118`: that layer has no aggregate state and schema-1 continuations are supported.
2. In `crates/orchestration/peritus-scheduler/src/durability.rs:116-208`, preserve `resolve_existing` before the new-live-genesis policy check. For a definitely absent command, read the aggregate head and checkpoint as already done. If both are absent, require a schema-2 `StartScheduler`; otherwise return a correct-input `BindingMismatch` such as “legacy scheduler schema cannot create a new durable aggregate.” If both exist, inspect the checkpoint's explicit family-72 header and require its schema-derived semantics to equal the command/event/successor semantics before append. Header inspection is the canonical discriminator, not a payload, time, provenance, or state-shape heuristic. The existing `HeadExpectation` and checkpoint compare-and-swap remain the race-closing write guard.

The second check is mandatory because `commit_scheduler_transition` is public and is used directly by daemon qualification and scheduler tests, not only through `SchedulerSession`; current call sites are visible at `durability.rs:108-114`, `crates/app/peritus-daemon/src/qualification/dependency/scheduler.rs:27-59,175-184`, and the scheduler durability/session suites. The first check improves the application result, while the second owns durable correctness.

Do not put the current-version-only rule in `start` at `crates/orchestration/peritus-scheduler/src/reducer.rs:17-58`. `replay` reconstructs the first command from the first event and calls `start` at `reducer.rs:112-136`; a current-only `start` would make every historical v1 journal unreadable. `start` remains the deterministic reducer for either explicitly decoded semantics. The live append boundary decides whether an absent aggregate may be created.

No database marker or rewrite is needed. Aggregate absence comes from the exact C0 head/checkpoint pair already used for append. Existing aggregate semantics comes from the checkpoint's versioned frame header. A malformed head/checkpoint presence pair remains an integrity error under the existing rule.

## Wire encoding and decoding

The present `CanonicalEncode`/`CanonicalDecode` traits have one associated `SCHEMA_VERSION`; `peritus-codec`'s generic `decode_message::<T>` rejects any other header version in `crates/foundation/peritus-codec/src/message.rs:47-54`. Therefore changing `SchedulerCommandFrame::SCHEMA_VERSION` to 2 is not a dual reader. Registry changes alone cannot make it one.

Implement private schema-specific wrappers for each family:

- `SchedulerCommandFrameV1`, `SchedulerEventFrameV1`, `SchedulerStateFrameV1` with schema 1 and legacy injection;
- the existing public `SchedulerCommandFrame`, `SchedulerEventFrame`, `SchedulerStateFrame` as the current schema-2 wrappers with strict injection.

Keep one shared payload writer/reader per family because the field order is unchanged. Pass `SchedulerSemantics` into `read_binding`, `SchedulerCommand::from_wire`, `SchedulerEvent::from_wire`, and state construction; never let the current public constructors silently supply semantics during historical decode.

Add public helpers in `crates/orchestration/peritus-scheduler/src/wire/mod.rs`:

```
encode_scheduler_command(&SchedulerCommand, CodecLimits) -> Result<Vec<u8>, CodecError>
decode_scheduler_command(&[u8], CodecLimits) -> Result<SchedulerCommand, CodecError>
encode_scheduler_event(&SchedulerEvent, CodecLimits) -> Result<Vec<u8>, CodecError>
decode_scheduler_event(&[u8], CodecLimits) -> Result<SchedulerEvent, CodecError>
encode_scheduler_state(&SchedulerState, CodecLimits) -> Result<Vec<u8>, CodecError>
decode_scheduler_state(&[u8], CodecLimits) -> Result<SchedulerState, CodecError>
```

Each encoder dispatches only from the value's explicit semantics. Each decoder first uses `decode_frame` to validate the complete 16-byte frame and inspect family/schema, then calls the corresponding schema-specific `decode_message`. That second header parse is bounded and preferable to changing the general codec API for one domain. It must reject wrong family, zero/unknown schema, nonzero flags, length mismatch, and trailing bytes with the existing codec classes.

The public schema-2 frame constructors retain their infallible source API. Their `encode_payload` implementations reject a wrapped legacy value with `WrongSchemaVersion` before emitting a payload, so generic `encode_message` cannot relabel it as schema 2. The wrapper is inert until encoding. Internal code uses the helpers. The frame-type exports remain available to current-schema clients, but their associated schema changes to 2; helpers are required for an aggregate whose version is discovered at runtime. This preserves constructor compatibility while enforcing the same encoding boundary as a fallible constructor.

Schema-1 encoding must reproduce every existing fixture byte, not merely decode to an equal value. Header family/version, payload layout, digests, event equality, and command request hashes remain exact.

## Canonical digests and durable identities

Branch canonical hashing on `SchedulerSemantics` in `crates/orchestration/peritus-scheduler/src/canonical.rs:15-63`:

- Legacy binding: retain domain `peritus-d3-scheduler-binding-v1\0` and its current preimage exactly.
- Legacy state: retain domain `peritus-d3-scheduler-state-v1\0` and its current preimage exactly.
- Strict binding: use domain `peritus-d3-scheduler-binding-v2\0`, write an explicit big-endian semantic tag `2`, then the existing binding fields.
- Strict state: use domain `peritus-d3-scheduler-state-v2\0`, write the same semantic tag `2`, then the existing state fields while logically zeroing the state-digest field as today.

The new domain is already separating, but retaining the semantic tag in the v2 preimage makes the binding/state contract self-describing and prevents a later refactor from dropping the distinction. Do not add any byte to the legacy preimage. `terminal_digest` remains under `peritus-d3-scheduler-terminal-v1\0`; terminal truth does not depend on admission semantics.

Command request identity in `crates/orchestration/peritus-scheduler/src/durability.rs:120-140` hashes the complete encoded command frame, so the schema header automatically separates otherwise identical v1/v2 commands. Event frames carry prior/successor state digests and have distinct headers. Strict E0 quality-cycle bindings receive the new binding digest; an existing v1 scheduler still produces its historical binding digest and therefore continues matching the E0 value already stored by `crates/app/peritus-daemon/src/authority/owner/orchestrator/children/scheduler.rs:38-43`.

Keep `AggregateKind::Scheduler`, `SCHEDULER_STATE_NAMESPACE = 0xD301`, and checkpoint-key domain `peritus.scheduler.state.v1\0` in `durability.rs:22-24`. These locate the aggregate and latest checkpoint; they are not reducer schema discriminators. Changing them would fork lookup and strand old data.

## Durability and aggregate stickiness

Replace direct generic scheduler frame calls in `crates/orchestration/peritus-scheduler/src/durability.rs:128-134,260-315` with the dual-version helpers. `SchedulerReplay` should retain `Option<SchedulerState>` or a decoded wrapper carrying state and schema, rather than a current-only `SchedulerStateFrame`. Rebuild continues to require exact replay/checkpoint equality.

On load:

1. Decode every event by its header version.
2. Require one semantics value across the event sequence.
3. Decode the checkpoint and require the same semantics.
4. Replay with that semantics and require exact state/checkpoint equality and existing C0 metadata equality.

On append, require command, event, predecessor state, and successor state to have one semantics value. Encode all three with that value. A schema-1 aggregate therefore appends schema-1 commands/events/checkpoints; it never upgrades merely because a dual reader is installed.

Exact idempotent command retry must still hash and look up the same versioned command bytes. A retry submitted with the wrong schema is a different request digest and must fail semantic binding rather than resolve the old command accidentally.

For a previously committed schema-1 genesis, resolution precedes the new-genesis rejection:

- Normal application retries are classified before domain dispatch. `crates/app/peritus-daemon/src/command/service.rs:44-65` returns an already settled application result or reconciles a pending/indeterminate record. It does not resubmit an `Existing` command to `domain::dispatch`.
- The application ledger retains `domain_command_digest = SHA-256(exact command frame)`, including the schema-1 header, at `command/service.rs:33-41,67-75`. Online reconciliation in `crates/app/peritus-daemon/src/authority/owner/storage.rs:11-42` and startup reconciliation in `crates/app/peritus-daemon/src/startup/recovery.rs:10-59` call C0 `resolve_command(command_id, domain_command_digest)`. The C0 query at `crates/state/peritus-journal/src/sqlite/query/command.rs:16-102` returns the immutable committed batch only for the exact identifier and digest.
- Direct D3 lost-acknowledgement retry continues through `resolve_existing` at `durability.rs:143-155,227-273` before checking whether a definitely absent command may create a legacy aggregate. If the checkpoint is still the reconstructed successor, it returns the recorded batch without append. If the aggregate has advanced, the current direct-D3 behavior remains `ReplayAggregate`; C0/application reconciliation can still return the original immutable batch without requiring the old checkpoint to be current.

Thus an exact old schema-1 genesis retry keeps its historical result. A schema-2 relabel has different complete-frame bytes and digest; the same command identity conflicts rather than aliases. A schema-1 genesis command absent from C0 and naming an absent aggregate is a new request and is rejected.

`crates/app/peritus-daemon/src/domain/dispatch.rs:133-168` must call `decode_scheduler_command`; its direct `decode_message::<SchedulerCommandFrame>` can only read one associated version. Scheduler session load/commit inherits the durability changes. The performance qualification encoder in `crates/app/testing/peritus-performance-qualification/src/scheduler.rs:7-129` and daemon runtime/conformance/recovery tests must use the scheduler helper so their request digest is computed from the exact selected version.

## Protocol registry and generated contracts

`MessageFamily` currently has only one `schema_version` in `crates/foundation/peritus-protocol/src/schema/registry.rs:8-19`, and families 70-72 are registered only at version 1 at lines 107-109. Represent current production and accepted historical versions separately:

```
pub struct MessageFamily {
    pub tag: u16,
    pub name: &'static str,
    pub schema_version: u16,              // current encoder version
    pub supported_schema_versions: &'static [u16],
    pub inert_only: bool,
}

pub const fn supports(self, version: u16) -> bool { ... }
```

Families 70-72 have current `2` and supported `[1, 2]`. Every other family has current `1` and supported `[1]`. Registry tests must require a nonempty, strictly increasing, duplicate-free list whose members are nonzero and which contains the current version.

Update the exact-current comparisons at:

- `crates/app/peritus-app-protocol/src/command/binding.rs:108-117`;
- `crates/app/peritus-app-protocol/src/subscription/frame.rs:33-49`;
- `crates/state/peritus-projection/src/replay.rs:169-182`;
- `crates/state/peritus-evidence/src/provenance.rs:143-152`.

They must call `supports(header.schema_version())`. This expands only the three scheduler families because every other support list remains `[1]`. Evidence `schema_digest(family, version)` already hashes family, requested version, and stable family name. Looking up by `supports(version)` preserves the existing v1 digest and creates a distinct v2 digest.

`crates/foundation/peritus-protocol/src/schema/render.rs:33-71` must emit both current and supported versions. In `protocol/generated/peritus-domain-v1.schema.json`, retain `schemaVersion` as the current version and add `supportedSchemaVersions`. In `protocol/generated/peritus-domain-v1.ts`, replace `FrameHeader.schemaVersion: 1` with a generated `MessageSchemaVersion` union (`1 | 2` for this registry) and export current/supported version data by family. The artifact name remains `peritus-domain-v1`: B3 framing format is still version 1; only three message schemas advance.

The registry establishes transport acceptance. It does not dispatch payload decoding: `CanonicalDecode::SCHEMA_VERSION` remains singular, so the scheduler-specific helpers are still required. Nor does the generic projection's registry check prove that all records in one scheduler aggregate use one semantic version. D3 durability/replay owns scheduler semantic validity and must reject mixed versions. Keep `peritus-projection` generic: it should accept either registered scheduler event schema as transport metadata and should not impose a global same-schema rule that could overconstrain future domain-defined migrations.

Regenerate the JSON/TypeScript artifacts with the existing code generator and keep exact-generation tests passing. Update `crates/foundation/peritus-protocol/tests/compatibility.rs`, `registry_roles.rs`, app-protocol subscription/command tests, projection replay tests, and evidence provenance tests for accepted scheduler v1/v2 plus rejected v0/v3.

## Historical fixtures, migration, and rollback

Do not overwrite `fixtures/protocol/scheduler-v1` or its SHA-256 manifest. Add `fixtures/protocol/scheduler-v2` and register it beside the current entry at `architecture.toml:70`. The v1 corpus should include fixed bytes for more than genesis: at minimum the benign sequence that dispatches retryable A, admits B, then succeeds A, and the recovery sequence whose checkpoint has two waiting records. Those fixtures prove legacy admission replay and the corrected legacy validation bound. If existing generated v1 fixtures contain only genesis, preserve them byte-for-byte and add immutable supplemental v1 cases.

No database migration or in-place checkpoint rewrite is needed. On the dual-reader release:

- every existing all-v1 aggregate decodes and replays under `LegacyQueueV1`;
- the formerly rejected recovery checkpoint is accepted only if it satisfies `W + Awork <= Q + A` and all other canonical/ownership checks;
- later commands for that aggregate remain v1;
- every newly created aggregate is v2 and strict.

The supported operational migration is to drain/finalize a legacy run and start a new schema-2 run. An in-place semantic upgrade would need a separately designed explicit event, a precondition `strict_pressure <= Q`, new binding/state digests, and coordinated replacement of the E0 binding digest. It is outside this repair and must not happen implicitly.

A binary that only understands schema 1 cannot read aggregates after schema-2 events are committed. The safe rollback floor is the new dual-reader revision. This branch should land dual-read support and schema-2 live genesis as one repository change; no deployment is performed here. An operator deployment may still stage binaries as needed, but that is outside GAP-03 implementation. Restoring a pre-deployment database snapshot would discard later committed data and is not a default rollback mechanism.

## Implementation surface

Scheduler domain and proofs:

- `crates/orchestration/peritus-scheduler/src/{lib.rs,semantics.rs,binding.rs,command.rs,event.rs,state.rs,projection.rs}`;
- verified clones/spec projections in the corresponding `clone.rs`/verified modules;
- `src/reducer.rs`, `src/reducer/fences.rs`, `src/reducer/apply.rs`, `src/reducer/apply/admission.rs`;
- `src/state/validation.rs` and active ownership validation in `src/verified.rs`;
- `src/canonical.rs` and fixed digest vectors;
- `src/wire/{mod.rs,command.rs,event.rs,state.rs,fixture_tests.rs}`;
- `src/durability.rs`, `src/durability/binding.rs`, and session/restart tests.

Cross-crate transport and runtime:

- protocol registry, renderer, generated JSON/TypeScript, and registry/codegen tests;
- app-protocol command binding and event subscription validation;
- state projection registry validation and evidence schema provenance;
- daemon scheduler dispatch and orchestrator child lifecycle command construction;
- daemon scheduler qualification, runtime/conformance/recovery-shutdown fixtures;
- performance qualification scheduler command generation;
- D3 README/collaboration documentation and `architecture.toml` fixture ownership.

Use `rg "SchedulerBinding::new|SchedulerCommand::new|Scheduler(Command|Event|State)Frame|decode_message::<.*Scheduler|encode_message.*Scheduler"` after implementation to find direct constructors and current-only generic codec calls. Current hits include scheduler support/fixture/replay/session/durability tests, daemon conformance/runtime/recovery tests, `authority/owner/orchestrator/children_tests/d3.rs`, `qualification/dependency/scheduler.rs`, and `crates/app/testing/peritus-performance-qualification/src/scheduler.rs`.

## Acceptance tests and proof evidence

Required behavioral tests:

1. Schema 2, `Q=1`: after dispatching recoverable A, admitting B is rejected before either worker loss or explicit failure, and rejected state is unchanged.
2. The same strict rejection for `RetrySafe`, `Ambiguous`, and `Fail` recovery policies when explicit retryable failure remains possible.
3. A final-attempt active item does not reserve strict pressure; `Cancelling` does not reserve strict pressure.
4. Worker-loss, explicit-failure, retry, dependency refresh, cancellation, exhaustion, abandonment, and completion each preserve or lower the exact schema-2 pressure relation.
5. Fixed v1 benign history replays even though schema-2 admission would reject its B event.
6. Fixed v1 recovery-overfill checkpoint decodes when `W + Awork <= Q + A`, replays exactly, and can accept a subsequent schema-1 command.
7. Malicious v1 checkpoints with `W <= Q + A` but `W + Awork > Q + A` are rejected. Include malformed/missing reservations to prove direct work-phase counting cannot be bypassed.
8. Malicious v2 checkpoints with pressure above Q are rejected for each omitted-category mutation: `WaitingDependencies`, `RetryPending`, recoverable `Reserved`, and recoverable `Running` under all three policies.
9. Attempts boundary is exact: `< maximum_attempts` counts and `== maximum_attempts` does not.
10. Mixed v1/v2 events, a checkpoint with a different version, wrong-family frames, schema 0, schema 3, nonzero flags, truncated frames, and trailing bytes are rejected.
11. V1 fixture encode is byte-identical and preserves binding/state/request/evidence digest vectors; v2 fixed vectors are distinct and stable.
12. Exact retry resolution works within each version and cannot alias across versions.
13. Registry, app-protocol binding, subscription, projection, evidence, durability restart, session cache, daemon dispatch, orchestrator pause/resume, and performance qualification all accept the intended supported versions and reject unsupported ones.
14. A fixed historical schema-1 genesis replays and rebuilds; a fresh raw schema-1 `StartScheduler` submitted through the daemon for an absent aggregate returns `UnsupportedSchema`; the same transition passed directly to D3 append is rejected and leaves head/checkpoint absent.
15. An exact already committed schema-1 genesis retry resolves through the application ledger/C0 without append. Cover settled and indeterminate application records, direct D3 lost acknowledgement while the genesis checkpoint is current, a same-ID schema-2 digest conflict, and the existing advanced-aggregate `ReplayAggregate` behavior.
16. A schema-1 continuation against a schema-1 checkpoint commits and remains schema 1. Schema-2 continuation against that checkpoint, schema-1 continuation against schema 2, and mixed event/checkpoint histories are rejected at D3 ownership boundaries.

Required verification relationships:

- exact `W`, `R`, and `Awork` classifiers and terminating count loop;
- schema-2 admission iff the ordered earlier checks pass and `P < Q`, with exact rejection priority and unchanged input;
- schema-1 admission retains its old `W < Q` decision;
- decoded validation selects exactly the semantic invariant and uses checked bounds;
- every production transition preserves its selected invariant, including every worker-loss/cancellation loop iteration and repeated recovery cycles;
- start/decide/event construction/durability binding/replay preserve one semantics value;
- pure start accepts explicit legacy replay while live absent-aggregate append requires current semantics after exact-command resolution;
- v1/v2 decoder selection is exactly header version and rejects unknown values;
- canonical v1 preimages are unchanged and v2 preimages include the selected semantic identity.

Negative proof probes should remove one active phase, restrict recoverable-active counting to `RetrySafe`, change `< maximum_attempts` to `<=`, restore the weak legacy `W <= Q + A` check, remove semantic equality from fences/durability, and route a schema-1 header through the schema-2 reducer. Each should fail a production-connected obligation, followed by byte-exact source restoration.

## Implementation defaults resolved for this branch

1. Add `SchedulerCommand::from_state` and semantic identity on `ProjectedScheduler`; do not add a speculative projection-only legacy command constructor. No in-repository projection-based scheduler client exists. Raw schema-1 continuations remain accepted at the wire boundary, and state-owning internal writers preserve semantics explicitly.
2. Enforce scheduler version stickiness in D3 replay/load and append, where scheduler state and checkpoint headers are available. Keep the generic projection limited to registered transport-version acceptance.
3. Land dual-read support, strict schema-2 creation, legacy continuation, generated registry changes, fixtures, tests, and proofs as one repository change. This task does not deploy it.
4. Keep pure `start` capable of both explicit semantics for deterministic replay. Reject a fresh live schema-1 genesis at daemon admission and, authoritatively, after exact-command resolution at D3 append.

These defaults preserve existing journals without a database rewrite and do not weaken schema-2 pressure or infer semantics from state contents.
