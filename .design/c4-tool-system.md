# Feature: C4 Tool System

- Status: frozen for implementation
- Date: 2026-08-24
- Owner: C4
- Crosslink issue: #13
- Depends on: B0, B1, B3, C0, C1, C2, C3, and A2 conformance support
- Unlocks: C5, C6, D0, D1, G0, and G3

## Summary

C4 adds the complete model-facing tool boundary to Peritus. Six crates define a stable tool
protocol, a target-owned authorization router, and built-in filesystem, Git, shell, and quality
tools. A model or provider may propose a call, but only the router can turn one exact prepared call
and matching committed B0/B1/C0 observations into a move-only invocation permit. Built-ins then
delegate effects to the already-authorized C1 workspace or C2/C3 process and sandbox boundaries.

The slice is production implementation, not a release or a reduced prototype. It delivers the
full inspect/edit/run/test tool surface needed by the later agent loop while leaving model-provider
transport to C5, context/roles to C6, durable agent orchestration to D0/E0, the complete gate DAG to
D1, and native packaged-host qualification to H2.

## User-visible behavior

A future model-provider adapter or agent loop can:

1. list the exact immutable tool descriptors exposed to one authenticated role and capability set;
2. submit a versioned call with structured JSON arguments, explicit bounds, an action identity,
   deadline, and idempotency identity;
3. receive schema errors before any authorization is consumed or effect occurs;
4. prepare the call into a deterministic digest-bound plan suitable for B0/B1/C0 authorization;
5. dispatch the plan only after the router validates exact committed lifecycle, capability, budget,
   revision, time, operation-class, and dispatch evidence;
6. receive ordered progress, structured success or failure, artifact references, bounded model and
   human renderings, timing, and truncation metadata;
7. control a running shell invocation with bounded stdin, PTY resize, signal, cancellation, and
   polling; and
8. retry an exactly identical idempotent call without duplicating its effect, while a conflicting
   replay is rejected explicitly.

Filesystem tools discover, inspect, search, read, create, replace, remove, and patch files. Git
tools report status, diff, and history and expose C1 candidate, snapshot, rollback, and approved
merge-related operations. Shell tools run structured argv by default and expose shell-script
parsing as a separately classified operation. Quality tools discover and invoke explicit B2-owned
checks and return evidence without claiming D1 acceptance.

## Requirements

### Protocol

1. `peritus-tool-protocol` owns schema version one for immutable descriptors and call, prepared,
   progress, result, artifact, failure, recovery, and control envelopes.
2. A descriptor contains a validated tool/capability name, semantic version, canonical schema and
   schema digest, the exact B1 `OperationDescriptor`, side-effect class, idempotency semantics,
   implementation identity, timeout/output limits, supported controls, and protocol compatibility.
3. Names, versions, text, JSON depth/member/string/byte counts, progress counts, artifact counts,
   output windows, and timeouts are bounded by constructors. Invalid states have no public
   constructor.
4. JSON is accepted only through the C4 bounded schema subset: object, array, string, integer,
   boolean, and null types; properties; required; additional-properties policy; items; enum;
   minimum/maximum; and string/array cardinality. Schema objects and call values are canonicalized
   recursively by UTF-8 key order before hashing.
5. Descriptor generation is deterministic. Equal semantic descriptors produce equal canonical
   bytes and digests on all supported platforms. Checked-in schema fixtures are generated from the
   canonical Rust descriptor builders and compared byte-for-byte.
6. Compatibility permits equal schemas and additive optional properties within the same major
   protocol/tool version. Removing or reinterpreting a field, changing a required property or
   operation class, or widening authority requires a new major version.
7. A result has a closed status distinct from prose. Success contains structured JSON. Failure
   contains a stable category/code, responsible subsystem, retryability, recovery route, and causal
   detail. Both contain bounded human/model renderings, artifact references, timing, and truncation.
8. Replay identity binds action ID, tool identity/version, descriptor digest, canonical argument
   digest, limits, and revision. An action ID with different bound bytes is a conflict.

### Router and authority

9. `peritus-tool-router` owns canonical descriptor registration, exact lookup, exposure planning,
   call preparation, target authorization, dispatch, active invocation control, result acceptance,
   bounded replay caching, cancellation, deadline enforcement, and recovery classification.
10. Registration rejects duplicate names, duplicate implementation identities, noncanonical order,
    descriptor/schema mismatch, and an operation name/class mismatch.
11. Exposure is the intersection of the registered descriptor, authenticated B1 operation
    registry, actor-role separation, and exact capability permissions. Input order cannot change
    the canonical exposed set.
12. Preparation validates the call envelope and complete argument value against the registered
    schema, computes canonical argument and prepared-call digests, and records the exact descriptor.
    Preparation is effect-free and consumes no authority.
13. `ToolAuthorizationRequest` borrows the exact `ActionIntentDto`,
    `CommittedKernelTransition`, `CommittedCapabilityUse`, `CommittedBudgetTransition`, optional
    `CommittedLeaseTransition`, `CurrentAuthorityEpoch`, revision, session, observed time, expected
    workspace generation/revision, and expected prepared-call digest. Its constructor is public and
    unprivileged; all fields are independently checked by the router.
14. The action intent media type is `application/vnd.peritus.tool-intent.v1`. Its canonical payload
    binds the action ID, descriptor digest, prepared-call digest, arguments digest, operation class,
    limits, and side-effect/idempotency declarations.
15. Successful authorization proves exact lifecycle/action/actor/role/environment/resource,
    capability, budget, optional lease, current authority epoch/time, revision, one-event B0
    dispatch receipt, descriptor, operation class, call, and prepared plan agreement.
16. The router alone constructs `AuthorizedInvocation`. It is public only so a dispatcher trait can
    consume it; its fields are private, it is neither `Clone` nor serializable, and it has no public
    constructor. Authorization consumes the action in a bounded replay ledger before the
    dispatcher can start.
17. `ToolDispatcher` reports one immutable implementation identity and descriptor digest. The
    router checks both against the registered/prepared call before invoking its only effectful
    method, which requires `AuthorizedInvocation` by value. Built-in crates expose no alternative
    raw effect method.
18. A dispatcher returns either a completed envelope or an owned `ToolExecution`. The router owns
    active executions and mediates poll, stdin, resize, signal, cancellation, deadline, terminal
    observation, and recovery. No spawned work is detached or silently dropped.
19. Active and completed tables are bounded. Exact replays return the prior terminal result only
    when descriptor idempotency permits it; non-idempotent calls report an explicit prior-outcome
    state and never repeat automatically. Conflicting or indeterminate replays fail closed.
20. Infrastructure failure, missing enforcement, output/artifact publication failure, timeout,
    cancellation, and indeterminate recovery remain non-success terminal categories.

### Built-in filesystem tools

21. `peritus-tools-fs` registers `fs.discover`, `fs.metadata`, `fs.read`, `fs.search`, `fs.create`,
    `fs.write`, `fs.remove`, `fs.replace`, and `fs.patch` with exact schemas and operation classes.
22. Read operations receive a C1 `ReadOnlyWorkspace`, accept only `WorkspacePath`, reject protected
    metadata and symlink traversal, remain inside the opened immutable root, and enforce entry,
    depth, file-size, search-match, and rendering bounds.
23. Mutating operations translate structured inputs into a C1 `PatchSet` with explicit preimage,
    generation, revision, mode, and line-ending policy, then invoke `WorkspaceGateway::apply_patch`
    with the matching `WorkspaceAuthorizationRequest`. Create, write, remove, and replacement are
    explicit patch forms; there is no direct ambient `std::fs` mutation path.
24. Binary reads and outputs use artifact references or explicit bounded base64/metadata forms.
    Search is literal or bounded regular-free matching in C4; future indexed semantic search may be
    added without changing the authorization boundary.

### Built-in Git tools

25. `peritus-tools-git` registers read-only status, diff, and history tools and authorized
    candidate, snapshot, rollback, and merge-related tools supported by the frozen C1 contract.
26. Read operations use `ReadOnlyWorkspace` and `peritus-git` structured observations. Small
    additive C1 APIs may expose structured diff/history observations; the tool must not parse an
    unrestricted user shell string or bypass C1 repository/worktree identity checks.
27. Candidate creation and rollback use `WorkspaceGateway` public methods with exact C1
    authorization requests. Candidate creation atomically retains the new C1 snapshot;
    `git.snapshot` inspects current/retained snapshot identity rather than inventing a second
    mutation lifecycle. Merge into a user branch remains unavailable until a later owner supplies
    the separately authorized C1 delivery operation; C4 reports it as unsupported rather than
    mutating refs directly.
28. Git output is structured by file/commit/ref identity and bounded independently from its human
    rendering. Non-UTF-8 names remain explicit byte-safe observations or typed unsupported errors.

### Built-in shell tools

29. `peritus-tools-shell` registers `shell.exec` for structured argv and `shell.script` for an
    explicit script/interpreter input. Both use B1 `Execution` so C2 can retain native restricted
    enforcement; their exact names, schemas, and risk sets differ, and `shell.script` cannot be
    reached through an argv flag or downgraded to an unrestricted raw-effect plan.
30. Preparation constructs a complete C2 `ExecutionPlan`: exact identity, argv, working directory,
    environment sources, stdin/PTY mode, deadline, output policy, sandbox/backend descriptor,
    resources, network request, secret delivery, and recovery metadata. There is no unrestricted
    fallback when platform enforcement is absent.
31. Dispatch receives a matching C2 `ExecutionAuthorizationRequest`, exact checked sandbox plan,
    backend admission, and concrete C3 native backend, and invokes only
    `ExecutionGateway::launch_with_backend`. The returned `OwnedProcess` is adapted into
    `ToolExecution`; all stdin, resize, signal, poll, cancellation, output, terminal, recovery,
    network, secret, and artifact behavior remains owned by C2/C3.
32. Complete bounded stdout/stderr is represented by C2 artifact observations while the model and
    human windows are separately truncated and labelled. Spawn failure, nonzero exit, signal,
    timeout, cancellation, sandbox denial, output overrun, and lost/indeterminate recovery are
    distinct result categories.

### Built-in quality tools

33. `peritus-tools-quality` registers `quality.discover` and `quality.run`. A check descriptor has
    a stable gate name/ID, source, structured argv, working directory, environment profile, timeout,
    output/parser bounds, and expected success rule.
34. Discovery reads explicit repository/project check definitions and known Cargo/Just surfaces;
    it does not silently invent acceptance policy. Results identify whether a definition is B2
    required, optional, or merely discovered.
35. Invocation compiles the selected check through the same C2 process/sandbox path as shell tools
    and returns a structured execution observation plus candidate B2 `GateObservation` inputs.
    C4 does not assert freshness or acceptance; D1 later binds the complete clean-snapshot gate DAG.
36. Infrastructure errors, missing tools, parser failures, cancellation, and timeouts remain
    explicit non-pass outcomes. Exit zero alone cannot erase an incomplete artifact or parser
    failure.

### Verification and maintainability

37. Verus owns deterministic bounds, canonical-order predicates, schema-shape predicates, exposure
    decisions, operation-class refinement, authorization fact conjunction, permit state,
    idempotency/replay transitions, lifecycle transitions, result acceptance, and no-effect-on-
    rejection properties. JSON parsing, dynamic dispatch, filesystem I/O, Git/process adapters,
    PTY/platform calls, and artifact I/O are narrow ordinary Rust shells with refinement tests.
38. `REF-C4-B1-OPERATION-CLASS` is discharged by an executable descriptor catalog and Verus
    predicate proving every built-in name maps to its authenticated B1 class and mandatory risk.
39. `REF-C4-B1-AUTHORITY-GATE` is discharged by the router authority proof, private constructor/API
    checks, exact receipt fixtures, one-use ledger, and integration tests proving no dispatcher is
    called after malformed, stale, forged, duplicate, or mismatched evidence.
40. Every crate root is composition-only and at most 80 lines. Source files stay at or below 400
    lines. No generic helper/common modules, catch-all errors, public dependency leakage, unsafe
    code, ignored tests, placeholder success, or reachable unfinished branch is permitted.

## Acceptance criteria

1. All six C4 crates are workspace members with READMEs, documented stable APIs, typed errors,
   strict workspace lints, architecture ownership, and locked dependencies.
2. Canonical descriptor/schema fixtures are byte-stable, schema digests verify, compatibility cases
   pass, and every built-in call accepts valid/minimal/maximal values and rejects malformed or
   over-limit values before dispatch.
3. Registry/exposure tests cover every B1 role and built-in operation class, order independence,
   unknown tools, duplicates, version mismatches, and capability intersection.
4. Router tests prove exact authority succeeds once and malformed, stale, wrong-action,
   wrong-digest, wrong-role, wrong-resource, wrong-revision, expired, missing-budget, incorrect
   lease, conflicting replay, and duplicate-use cases invoke no dispatcher.
5. Exact idempotent replay returns the recorded terminal result; non-idempotent and indeterminate
   retry cases do not repeat effects.
6. Active execution tests cover ordered progress, poll, stdin, PTY resize, signals, cancellation,
   deadline, output truncation, terminal publication, dispatcher failure, dropped caller, and
   recovery classification.
7. Filesystem integration tests use fresh temporary workspaces and real C1 gateways for bounded
   discovery/read/search and atomic create/write/remove/replace/multi-file patch, including
   preimage conflict, protected path, symlink, generation, lease, and rollback behavior.
8. Git integration tests use fresh temporary repositories/worktrees and structured C1 operations
   for status, diff, history, candidate, snapshot, and rollback. No raw ref mutation path is public.
9. Shell integration tests use the real C2 gateway and native C3 backend available on the host for
   argv, pipes, stdin, PTY/resize, signal, cancellation, timeout, nonzero exit, bounded output,
   artifact publication, resources, network/secret policy, and unsupported enforcement.
10. Quality integration tests discover and invoke real temporary-project checks, return structured
    evidence, and never turn infrastructure/parser failure into pass.
11. `peritus-conformance` supplies a nonempty `tool_suite`; every case receives a fresh subject and
    covers descriptors, schema, exposure, authorization, dispatch, result truth, cancellation, and
    idempotency.
12. Focused tests, all workspace checks, architecture/source/API/trust/reproducibility checks,
    dependency policy, toolchain checks, no-cheating Verus verification/build, relevant platform
    compilation, and full local Gate A pass serially with the requested job limits.
13. README and `docs/c4-tool-system.md` accurately describe the implemented surface, known hosted
    runner limitation, remaining packaged-host qualification, and the next canonical slice.
14. A signed source commit is reviewed, pushed, merged through a PR, remains an ancestor of main,
    and the active repository ruleset is restored to 22 required checks with no bypass actors.

## Current architecture

B1 owns `OperationDescriptor`, role separation, capability use, budgets, lease semantics, and the
two C4 refinement reservations. B0 owns exact action lifecycle and dispatch witnesses. B3 owns
`ActionIntentDto` and canonical action hashing. C0 returns opaque committed kernel, capability,
budget, and lease observations plus the current authority epoch.

C1 owns `WorkspaceAuthorizationRequest`, `WorkspaceGateway`, `ReadOnlyWorkspace`, `WorkspacePath`,
checked `PatchSet` application, Git repository/worktree identity, candidates, snapshots, rollback,
and reconciliation. C2 owns `ExecutionAuthorizationRequest`, `ExecutionPlan`, `ExecutionGateway`,
`OwnedProcess`, process controls, output spooling, terminal observations, cancellation, and recovery.
C3 supplies native sandbox, network, and secret implementations through C2 seams. B2 owns gate and
acceptance semantics. A2 supplies deterministic subjects, temporary repositories, and the generic
fresh-subject conformance runner.

No C4 crate exists yet. The A2 `tool_suite` is intentionally empty, and README names C4 as the next
runtime boundary.

## Proposed design

### Crate and module layout

All new crates live below `crates/tools/`:

```text
peritus-tool-protocol/src/
  lib.rs identity.rs schema.rs schema/{canonical,compatibility,validate}.rs
  descriptor.rs call.rs prepared.rs progress.rs result.rs artifact.rs
  error.rs limits.rs idempotency.rs control.rs verified.rs
peritus-tool-router/src/
  lib.rs registry.rs exposure.rs preparation.rs authorization.rs intent.rs
  dispatch.rs dispatcher.rs execution.rs replay.rs cancellation.rs
  recovery.rs error.rs verified.rs
peritus-tools-fs/src/
  lib.rs catalog.rs input.rs read.rs search.rs metadata.rs patch.rs
  dispatcher.rs render.rs error.rs
peritus-tools-git/src/
  lib.rs catalog.rs input.rs status.rs diff.rs history.rs candidate.rs
  snapshot.rs rollback.rs dispatcher.rs render.rs error.rs
peritus-tools-shell/src/
  lib.rs catalog.rs input.rs plan.rs dispatcher.rs execution.rs render.rs error.rs
peritus-tools-quality/src/
  lib.rs catalog.rs definition.rs discovery.rs input.rs plan.rs parser.rs
  dispatcher.rs observation.rs render.rs error.rs
```

Files are split further before crossing 400 lines. Roots contain module declarations and public
re-exports only.

### Two-phase call lifecycle

```text
ToolCall
  -> registry lookup + bounded schema validation
  -> PreparedToolCall (effect-free, digest-bound)
  -> E0 commits matching B1/B0/C0 transitions
  -> ToolAuthorizationRequest + prepared call + bound dispatcher
  -> router exact validation + one-use ledger
  -> AuthorizedInvocation (private construction)
  -> C1/C2-backed dispatcher
  -> Completed ToolResult | owned active ToolExecution
  -> progress/control/poll/recovery
  -> terminal ToolResult + replay record
```

Preparation can be repeated because it has no effects. Dispatch cannot be repeated merely because
a client timed out. The replay policy determines whether an exact terminal result may be returned;
it never recreates a permit for a second effect.

### Descriptor catalog and operation refinement

Tool names are also B1 capability names. Every built-in defines one `OperationDescriptor` with the
mandatory risk and any additional risks. The descriptor catalog is canonical and checked both when
constructed and when registered. The built-in mapping is:

| Family | Operations | B1 class |
|---|---|---|
| filesystem inspect | discover, metadata, read, search | `Inspection` |
| filesystem mutate | create, write, remove, replace, patch | `WorkspaceMutation` |
| Git inspect | status, diff, history | `Inspection` |
| Git candidate/snapshot/rollback | exact C1 mutations | `WorkspaceMutation` |
| Git protected ref/history mutation | only when a C1 delivery API exists | `RepositoryHistoryMutation` |
| shell argv | exec | `Execution` |
| shell script | script | `Execution` with a distinct schema/name and additional risk |
| quality discover | discover | `Inspection` |
| quality invoke | run | `Execution` |

The implementation identity includes the built-in family, crate version, protocol major, and
descriptor catalog digest. Registration and dispatch compare it exactly.

### Dispatcher boundary

`ToolDispatcher` is object-safe. It exposes immutable identity queries and one start method:

```text
descriptor_digest() -> SchemaDigest
implementation_identity() -> &ImplementationIdentity
start(AuthorizedInvocation) -> Result<ToolStart, DispatchFailure>
```

`ToolStart` is either a terminal result or an owned execution implementing poll/control/cancel and
terminal extraction. A built-in dispatcher constructor binds the exact lower-layer handles and
authorization request needed by that family. The router compares the dispatcher identity before it
creates and moves the permit. Concrete built-ins do not expose an effectful method that omits the
permit.

Filesystem mutation dispatchers bind `&mut WorkspaceGateway` and
`&WorkspaceAuthorizationRequest`. Read dispatchers bind `&ReadOnlyWorkspace`. Git mutation
dispatchers additionally bind exact candidate/snapshot/rollback inputs. Shell and quality run
dispatchers bind `&ExecutionGateway`, the matching `ExecutionAuthorizationRequest`, the prepared
C2 restricted plan, exact checked sandbox plan/backend admission, and a concrete C3 native backend.
Returned C2 processes are owned by the running execution adapter.

### Error and rendering policy

Protocol errors are stable domain values. Adapter errors retain their C1/C2/C3 source category and
map to a C4 subsystem/category without discarding the original stable code. Prose is diagnostic,
bounded, and never used to determine success. Renderers escape control characters and label
truncation. Full output is referenced through artifacts when present.

### Conformance

The A2 tool subject models descriptor listing, preparation, authorization, dispatch observation,
progress/control, terminal result, and replay. Cases create a new subject for each run and never
reuse state accidentally. Production C4 adapters implement the subject using real registries and
fixture dispatchers; focused built-in integration tests exercise C1/C2 directly.

### Alternatives considered

A JSON-only router that returned a tool name to a caller-owned switch was rejected because it
could validate presentation data but could not prove which implementation received the authorized
call. A second C4 policy engine was rejected because B1 already owns operation classification,
roles, capabilities, budgets, and leases; duplicating those decisions would create drift. Storing
heterogeneous C1/C2 references inside the router was also rejected because it would couple the
verified routing core to every effect implementation and create cyclic tool dependencies.

The selected bound-dispatcher seam keeps descriptors and routing uniform, verifies the exact
implementation identity immediately before dispatch, makes the router permit mandatory, and lets
each built-in retain its concrete lower-layer authority types without erasure or ambient access.

### Parallel ownership

After this document is frozen, three implementation lanes may run concurrently:

1. Protocol/router owns only `crates/tools/peritus-tool-protocol/` and
   `crates/tools/peritus-tool-router/`.
2. Workspace tools owns only `crates/tools/peritus-tools-fs/`,
   `crates/tools/peritus-tools-git/`, and explicitly required additive structured read APIs under
   `crates/runtime/peritus-git/` or `crates/runtime/peritus-workspace/`.
3. Execution/quality owns only `crates/tools/peritus-tools-shell/`,
   `crates/tools/peritus-tools-quality/`, and explicitly required additive adapter APIs under
   `crates/runtime/peritus-process/`.

The root owner exclusively edits workspace manifests, lockfile, `architecture.toml`, `justfile`,
verification manifests, A2 conformance/test-support, generated cross-crate assets, design/docs,
README, CI/governance files, and integration tests outside lane-owned directories. Lanes must not
edit another lane or root-owned paths. Cross-lane contract corrections are messaged to the root and
made once during integration.

## Data and compatibility

Protocol/schema/tool major version one becomes compatibility-sensitive when C5/D0 persists or
transports it. Before that consumer lands, C4 may still correct an implementation mistake in the
same feature branch. After merge, incompatible changes add a major version and retain prior decode
and compatibility fixtures; stored bytes are never reinterpreted.

Canonical hashing uses exact canonical JSON/schema bytes and domain-separated prepared/intent
bytes. Tool results preserve the exact descriptor and replay digests that produced them. Artifact
references contain digest, size, media type, completeness, and provenance rather than filesystem
paths.

## Failure handling

- Invalid descriptor, schema, call, or bounds: reject before authority or effect.
- Unknown/hidden tool: return a stable exposure error without revealing inaccessible descriptors.
- Authority mismatch or stale evidence: consume no dispatch permit and call no dispatcher.
- Local replay-ledger persistence ambiguity: mark the call indeterminate and require recovery; do
  not repeat the effect.
- Dispatcher identity mismatch: reject before effect.
- C1/C2 rejection: preserve the lower stable category and report non-success.
- Started process loses observation: report indeterminate/lost, retain recovery identity, and never
  infer success from disappearance.
- Artifact publication failure: terminal result is incomplete/infrastructure failure even if the
  underlying command exited zero.
- Cancellation/deadline: request graceful C2 stop, escalate according to C2 policy, observe the
  terminal process, and return cancelled/timed-out rather than success.
- Router restart: replay records and C2/C3 recovery observations decide completed, active,
  retryable-before-effect, or indeterminate. Memory loss alone never authorizes a repeat.

## Security considerations

The model supplies untrusted JSON only. Schema validation, bounds, role/capability exposure, exact
committed authority, and lower target gateways remain independent checks. A tool name or rendered
description grants no authority. Built-ins cannot construct router permits or C1/C2 permits.

Filesystem paths use C1 `WorkspacePath`, immutable/writable handles, protected-component checks,
and no-follow resolution. Shell input never becomes a script unless the separately classified
`shell.script` tool was authorized. Environment variables and secrets use C2/C3 typed sources;
secret material is not returned in tool envelopes. Network behavior remains denied unless the C2/
C3 plan authorizes it. Git protected refs and user branches are not mutated without a dedicated
lower-layer operation.

This slice tests realistic trust-boundary failures. It does not spend implementation time modeling
attackers with arbitrary in-process native code, kernel compromise, or physical host control; such
actors already exceed the local harness trust model.

## Verification

Focused commands are:

```text
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-tool-protocol -p peritus-tool-router \
  -p peritus-tools-fs -p peritus-tools-git -p peritus-tools-shell -p peritus-tools-quality \
  --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-conformance --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- architecture-check
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- source-layout-check
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- ordinary-api-check
CARGO_BUILD_JOBS=2 cargo deny --locked check
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- toolchain-check
CARGO_BUILD_JOBS=1 just verus-verify
CARGO_BUILD_JOBS=1 just verus-build
CARGO_BUILD_JOBS=1 just gate-a
```

Focused test runs precede the full gate. Heavy external builds never overlap. Relevant Linux native
tests and Windows/macOS compilation checks run after ordinary integration passes.

Every requirement above maps directly to an acceptance criterion. `verification/obligations.toml`
records the two C4 refinements with exact symbols and test commands; architecture no longer lists
them as future reservations after the implementation and evidence exist.

## Rollout and rollback

C4 lands in one signed protected-main PR containing all six crates, shared conformance, governance,
proof inventory, generated fixtures, documentation, and validation evidence. It is not advertised
as a user-facing release.

Before C5/D0 consume the protocol, rollback can remove the additive C4 crates and registrations.
After a downstream consumer persists version-one calls/results, rollback retains the protocol
decoder and explicit unsupported dispatcher behavior so historical evidence remains readable.

Hosted jobs that execute zero steps because of the known account restriction are recorded as
unavailable, not passed. Any temporary merge-rule relaxation is restored exactly to the active 22
checks with no bypass actors immediately after merge.

## Open questions

None. Interface, ownership, security, and slice boundaries are frozen for implementation.

## Out of scope

- C5 provider adapters and provider streaming normalization.
- C6 role-context construction and memory selection.
- D0 durable model/tool turn orchestration and E0 authority assembly.
- D1 complete gate DAG, clean-snapshot freshness, acceptance aggregation, and gate dependency
  scheduling. C4 supplies the invocation/evidence boundary D1 consumes.
- G0 daemon composition and public CLI/TUI/API surfaces.
- G3 MCP/plugin transport and isolation; those implementations reuse the C4 protocol/router.
- H0 independent release security review, H1 load/soak qualification, H2 packaged-host native
  qualification, H3 representative campaigns, and H4 release assembly.
- Direct merge into a user branch until the dedicated lower C1 delivery boundary exists.

These exclusions assign later architecture ownership; they do not reduce any C4 behavior described
in the requirements or acceptance criteria.
