# C4 tool system

C4 is Peritus's model-facing inspect, edit, run, and check boundary. It consists of six internal
libraries rather than a CLI or daemon:

- `peritus-tool-protocol` defines bounded descriptors, schemas, calls, progress, controls, results,
  failures, artifacts, and replay identities;
- `peritus-tool-router` owns registration, exposure, schema validation, preparation, committed
  authority validation, one-use dispatch, active executions, deadlines, cancellation, recovery,
  and replay;
- `peritus-tools-fs` implements structured filesystem inspection and patch-backed mutation;
- `peritus-tools-git` implements structured repository inspection and authorized C1 candidate,
  snapshot, and rollback operations;
- `peritus-tools-shell` adapts structured argv and separately classified script requests to the
  restricted C2/C3 process boundary; and
- `peritus-tools-quality` discovers and invokes explicit project checks without deciding
  acceptance.

These crates are the runtime tool substrate consumed by later provider, agent-loop, gate, daemon,
and extension slices. C4 itself does not expose a user command and does not claim product or native
packaged-host qualification.

## Call lifecycle

Every invocation is deliberately two-phase:

```text
untrusted ToolCall
  -> bounded JSON and exact schema validation
  -> PreparedToolCall with canonical digests
  -> B1/B0/C0 authorization and durable dispatch outside C4
  -> ToolAuthorizationRequest
  -> router cross-check and one-use replay consume
  -> opaque AuthorizedInvocation
  -> exact implementation-bound ToolDispatcher
  -> terminal ToolResult or owned ToolExecution
```

Preparation is effect-free. It resolves the descriptor, validates the tool version and arguments,
and binds the descriptor, schema, arguments, limits, revision, deadline, and idempotency key into a
prepared/replay identity. A malformed or over-limit call stops there.

The unprivileged `ToolAuthorizationRequest` borrows the matching domain action intent, committed
kernel dispatch, capability use, budget reservation, optional lease use, current authority epoch,
revision/session, observed time, workspace generation/revision, and prepared-call digest. The
router checks the complete bundle rather than trusting its constructor.

Only a successful check constructs `AuthorizedInvocation`. The value is move-only, has private
fields, and cannot be deserialized. A dispatcher can start only by consuming it. The router checks
the dispatcher's immutable implementation identity and descriptor digest immediately before that
move, so a caller cannot validate one tool and invoke another.

## Protocol and schemas

Tool names use the same validated hierarchical names as B1 capabilities. Semantic versions,
implementation identities, user/model text, JSON values, nesting, object members, arrays, strings,
timeouts, output windows, artifacts, and progress are bounded domain types.

C4 supports the JSON-schema surface required by built-ins: object, array, string, integer, boolean,
and null types; properties and required members; additional-property policy; items; enum; numeric
bounds; and string/array cardinality. Objects are rendered recursively in canonical UTF-8 key order
before digesting. A descriptor carries its schema digest and exact B1 `OperationDescriptor`.

Within one major version, compatibility accepts an equal schema or additive optional properties.
Required-field, authority, or semantic changes require a new major version. Deterministic fixtures
and compatibility tests prevent accidental reinterpretation.

## Exposure and operation classes

Registration is canonical and duplicate-free. Exposure is the intersection of:

1. a registered tool descriptor;
2. the authenticated B1 operation registry;
3. compiled role separation; and
4. the actor's exact capability permissions.

Tool descriptions and model output never participate in authority. The built-in mapping is:

| Tools | B1 operation class |
|---|---|
| filesystem discovery, metadata, read, search | `Inspection` |
| filesystem create, write, replace, remove, patch | `WorkspaceMutation` |
| Git status, diff, history | `Inspection` |
| Git candidate, snapshot, rollback | `WorkspaceMutation` |
| structured argv and separately named script execution | `Execution` |
| quality discovery | `Inspection` |
| quality invocation | `Execution` |

Script execution retains restricted native sandbox enforcement. It is a distinct descriptor,
schema, capability name, and risk set; it is never smuggled through an argv flag and never falls
back to C2's explicit raw-effect launch.

## Filesystem tools

Read operations begin from a C1 immutable `ReadOnlyWorkspace` and accept only checked
`WorkspacePath` values. They reject traversal, platform aliases, protected `.git`/`.peritus`
components, symlink following, excessive depth/entries, oversized files, and excessive matches.
Results are structured and independently bound their model/human rendering.

Mutations do not write through ambient filesystem APIs. Create, write, replace, remove, and patch
requests become explicit `PatchSet` values containing expected workspace identity, generation,
revision, preimages, mode, line-ending policy, and final content. The dispatcher then calls
`WorkspaceGateway::apply_patch` with the exact C1 `WorkspaceAuthorizationRequest`. C1 retains
atomicity, rollback, transaction recovery, mutation condition, and action-consumption ownership.

## Git tools

Status, diff, history, and snapshot inspection use structured C1 observations associated with a
checked repository/worktree identity. Candidate creation and rollback use the public C1 workspace
gateway with exact authorization; candidate creation also returns C1's retained snapshot identity.

C4 does not synthesize shell commands for Git authorization and does not mutate protected refs.
Merging into a user branch remains unavailable until the later dedicated C1 delivery boundary
exists; an unsupported result is preferable to bypassing that ownership rule.

## Shell tools

`shell.exec` represents executable plus argv literally. `shell.script` represents an explicit
interpreter/script request with a separate capability and risk profile. Both prepare a complete C2
restricted `ExecutionPlan`, including identity, working directory, environment sources, I/O mode,
deadline, output policy, resources, sandbox/backend identity, network policy, secret delivery, and
recovery information.

The production dispatcher binds the exact `ExecutionAuthorizationRequest`, checked sandbox plan,
backend admission, and concrete native C3 backend before calling
`ExecutionGateway::launch_with_backend`. Unsupported native enforcement is a failure; it cannot
select unrestricted execution.

The returned `OwnedProcess` remains C2-owned. Its C4 execution adapter mediates ordered polling,
stdin, PTY resize, signals, cancellation, deadline escalation, terminal observation, output and
artifact publication, and restart classification. Spawn failure, nonzero exit, signal, timeout,
cancellation, sandbox denial, output overrun, publication failure, and indeterminate recovery are
different structured outcomes.

## Quality tools

`quality.discover` combines explicitly supplied typed check definitions with deterministic Cargo
and Just command-surface discovery at an opened project root. C4 does not invent an ambient project
configuration filename or silently promote discovered commands into acceptance requirements.
Every returned definition records its source and whether B2 policy requires it.

`quality.run` selects one known definition, prepares it through the same restricted C2/C3 path as
shell execution, and returns process/artifact facts plus candidate gate-observation inputs. It does
not claim that the result is current or sufficient for acceptance. D1 later owns clean-snapshot
freshness, dependencies, parser policy, DAG scheduling, and aggregation.

Missing executables, infrastructure failures, parser failures, incomplete artifacts, cancellation,
and timeouts never become pass merely because a child exit code was zero.

## Results, controls, and replay

Every terminal envelope has a closed success/failure status. Success contains structured JSON;
failure contains a stable category and code, responsible subsystem, retryability, recovery route,
and causal detail. Human and model renderings are bounded views, never the source of truth. Artifact
references, timing, and truncation metadata are carried separately.

Active executions are router-owned and bounded. Progress sequence numbers are monotonic. Stdin,
resize, signal, cancel, deadline, and recovery calls are mediated against the exact active action.
Dropping a client does not detach the owned execution.

An exact idempotent replay may return the already-recorded terminal result without a second effect.
A conflicting action identity, a non-idempotent retry, or an indeterminate prior outcome never
constructs a fresh permit. Infrastructure ambiguity remains visible.

## Verification

The C4 proof and refinement surface covers canonical bounds/order, schema shape, exposure,
operation-class agreement, complete authorization facts, one-use permit state, execution lifecycle,
result acceptance, and replay transitions. Ordinary Rust owns JSON parsing, dynamic dispatch, and
C1/C2/C3 effects behind checked inputs and direct integration tests.

`OBL-0135` implements `REF-C4-B1-OPERATION-CLASS` through the router's checked operation-refinement
predicate and concrete catalog registration. `OBL-0136` implements `REF-C4-B1-AUTHORITY-GATE`
through the router's complete-authority predicate, private consuming permit, rejection no-effect
tests, and production A2 conformance adapter. Both remain recorded as in-progress proof obligations
until the repository's independent final proof review; the architecture reservations themselves are
resolved by this slice.

The reusable A2 `tool_suite` is nonempty and creates a fresh subject per case. It exercises
descriptor/schema determinism, invalid-schema no-effect behavior, exposure, exact one-use dispatch,
independent authority drift, truthful result status, cancellation/deadline ownership, and replay.

Focused development checks are:

```text
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-tool-protocol -p peritus-tool-router \
  -p peritus-tools-fs -p peritus-tools-git -p peritus-tools-shell -p peritus-tools-quality \
  --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-conformance --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- ordinary-api-check
CARGO_BUILD_JOBS=1 just verus-verify
CARGO_BUILD_JOBS=1 just verus-build
```

`just gate-a` remains the complete merge authority. Heavy commands run serially. Hosted workflow
runs that execute zero steps because of the account-level runner restriction are unavailable
evidence, not passing evidence.

## Remaining boundaries

C4 supplies libraries, not a complete agent. C5 adds provider-neutral model streaming and provider
adapters. C6 adds role/context/memory selection. D0 then composes the durable model/tool loop, and
D1 consumes the quality invocation surface for the full gate engine. Native packaged-host parity
and independent qualification remain H2 work.
