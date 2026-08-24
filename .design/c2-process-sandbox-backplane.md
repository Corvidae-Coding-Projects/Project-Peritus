# Feature: C2 process and sandbox backplane

## Summary

C2 supplies the owned execution boundary for Project Peritus. It adds two class-H runtime crates:
`peritus-process` and `peritus-sandbox`. Together they compile complete platform-neutral sandbox
contracts, bind those contracts to structured process plans, authorize one exact committed action,
launch real pipe or PTY processes without a shell, own the resulting process tree and support
tasks, stream bounded events and output, enforce supervisor-owned limits and cancellation, persist
restart state, and report one deterministic terminal disposition.

`peritus-process` owns the sole execution effect gateway. The gateway validates the exact B0
dispatch, B1 capability, budget, and lease state where required, B3 action bytes, C0 committed
receipts and current epoch, process owner lineage, resource, environment, revision, generation,
current authority time, sandbox backend fingerprint, and complete execution-plan digest immediately
before effect. Only that gateway constructs its crate-private one-use permit. There is no public raw
spawn path, shell-string shortcut, caller-created permit, or authorization boolean.

`peritus-sandbox` owns inert checked policy and backend-preparation values. It represents every
filesystem, process, environment, network, secret, resource, and terminal capability required by
the production product, admits a backend only when it supports the complete plan, and supplies a
real executable reference backend for semantic and conformance testing. C3 owns native
Linux/macOS/Windows enforcement, managed network, and secret injection. That division does not
defer any C2 contract or reference behavior: C3 implements the complete interfaces frozen here and
may fail closed, but may not reinterpret or weaken them.

Deterministic construction, canonicalization, authorization comparison, lifecycle reduction,
output accounting, backend admission, recovery classification, and holder-quiescence decisions are
executable Verus Rust. Process creation, PTY calls, process-group/job-object control, clocks,
filesystem durability, resource probes, and artifact transfer remain narrow ordinary-Rust effect
shells with bounded observations.

## User-visible behavior

1. A process request names one executable and structured argv. Arguments are passed literally and
   are never interpolated by a shell.
2. The child starts in one checked working directory with one deterministic clear-and-set or
   allowlisted environment. Ambient credentials are not inherited accidentally.
3. Callers select separate stdout/stderr pipes or a PTY. They can write bounded stdin, close input,
   resize an allowed terminal, request cancellation, and read ordered bounded events.
4. Every spawned root and descendant is owned by one project/session/run/attempt/turn/action and
   one `ProcessId`. The supervisor retains the wait, input, output, cancellation, and backend
   lifecycle tasks until all have stopped and joined.
5. Deadlines, output ceilings, explicit cancellation, lease fencing, backend failure, and resource
   limits terminate the owned tree. Graceful stop and forced escalation remain distinguishable.
6. One terminal result distinguishes normal exit, signal or platform exception, cancellation,
   timeout, output/resource limit, sandbox denial, supervisor failure, and recovery indeterminacy.
   It also reports the underlying OS observation, output completeness, escalation, and resources.
7. Output is bounded in memory and on disk. Retained stdout, stderr, and terminal streams can be
   finalized into C0 artifacts without treating publication failure as process success.
8. Restart reopens durable execution records and classifies each as already terminal, exact live
   owned tree, absent without a terminal observation, or indeterminate. It never guesses success or
   signals an unrelated reused process identity.
9. Sandbox requests are default-deny and explicit across all seven capability domains. A missing
   required enforcement feature returns a typed unsupported result before process launch.
10. The reference sandbox backend executes the full abstract policy, lifecycle, accounting,
    observation, and cancellation semantics. It is a conformance oracle, not a fake success stub
    and not a claim of native host isolation.
11. After every process and support task for an exact prior lease holder is terminal and joined,
    C2 can issue correlated holder-quiescence evidence. Any live, unresolved, or incomplete record
    blocks that evidence.

## Requirements

### R-C2-001 — structured command and checked target

`CommandSpec` contains one bounded nonempty executable and a bounded ordered argument vector. It
rejects NULs, empty executable names, oversized values, excessive arguments, and an empty argv
identity. It has no shell parser and no constructor accepting a command line string.

`WorkingDirectory` is opened and canonicalized before authorization, must be a directory, and is
bound to the execution target's `WorkspaceId`, `ResourceId`, generation, revision, and environment.
The process adapter receives the checked directory value, never an independently supplied later
path. C3 strengthens platform handle and alias semantics without changing this identity.

### R-C2-002 — deterministic environment

`EnvironmentPlan` is either `Cleared` or an explicit inheritance allowlist plus literal bindings.
Names use a bounded portable ASCII form, values are bounded byte strings representable by the host,
collections are canonical, and case-fold collisions are rejected. Explicit bindings override only
names authorized by the sandbox contract. Process launch starts from `env_clear` and adds exactly
the checked result.

### R-C2-003 — complete execution plan identity

`ExecutionIdentity` binds project, session, run, attempt, turn, action, process, workspace,
resource, environment, and exact `RevisionTuple`. `ExecutionPlan` additionally binds command,
working directory, environment, pipe/PTY mode, terminal size, input/output policy, deadline and
escalation policy, resource ceilings, workspace access mode, abstract sandbox digest, selected
backend identity/version/support digest, and expected preparation digest.

Canonical bytes use a versioned domain separator, fixed discriminants, big-endian lengths and
integers, complete field coverage, and bounded canonical collections. `ExecutionPlan::digest`
hashes those exact bytes. `ExecutionIntentPayload` is the exact B3 action payload and binds the
process ID, execution-plan digest, sandbox digest, and backend descriptor digest. The media type is
`application/vnd.peritus.execution-plan.v1`.

### R-C2-004 — complete target-owned authorization gateway

`ExecutionAuthorizationRequest` is unprivileged borrowed input. It contains the action intent,
`CommittedKernelTransition`, `CommittedCapabilityUse`, `CommittedBudgetTransition`, optional
`CommittedLeaseTransition`, `CurrentAuthorityEpoch`, exact session, revision, expected workspace
generation/revision, authority observation, and exact plan digest.

Immediately before effect, `ExecutionGateway` validates:

- the intent media type and exact `ExecutionIntentPayload` bytes;
- `OperationClass::Execution` for restricted execution, or `RawEffect` only for a contract that
  explicitly requested raw effect with separately matching capability;
- the role's compiled operation permission;
- one exact committed `ActionDispatched` event frame and its command/event identities;
- the B0 action, authorization witness, action digest, actor, role, environment, resource, and
  complete parent project/session/run/attempt/turn lineage;
- the B1 capability permission, action binding, scope, revision, time state, transition digest,
  and C0 committed capability receipt;
- one committed B1 budget `Begin` reservation for the same action, digest, revision, and an
  adequate active-effect ceiling;
- for `Writable` workspace access, one committed active lease use for the same action, claim,
  scope, actor/session holder, generation, version, environment, capability, and unexpired time;
- for `ReadOnly` access, absence of surplus mutation-lease authority and a read-only target binding;
- exact current authority epoch and half-open capability/lease validity at `observed_at`;
- every `RevisionTuple` component, expected generation/revision, plan target, sandbox digest,
  backend descriptor/support digest, and preparation digest;
- no durable prior consumption of the action/process pair.

Only after all comparisons succeed does the gateway atomically persist consumption and execution
intent, construct a crate-private move-only `ExecutionPermit`, and immediately consume it into an
opaque `AuthorizedLaunch`. A rejection invokes neither sandbox activation nor process launch.
Failure after durable consumption cannot replay the authorization.

This rule discharges `REF-C2-B1-AUTHORITY-GATE`.

### R-C2-005 — sandbox contract coverage

`SandboxContract` contains one complete checked value for each domain:

- filesystem discovery, metadata, read, execute, create, write, and remove rules;
- root executable, descendant spawning, signal/control, process-tree containment, and process
  count;
- cleared/allowlisted environment names and literal/secret delivery destinations;
- outbound network denial or exact DNS/IP/protocol/port rules, with inbound denied;
- secret references and delivery by environment, file, or brokered handle, never secret values;
- wall, CPU, memory, disk, output, file-descriptor/handle, process, and concurrent-slot limits;
- pipes/PTY, stdin, resize, terminal signals, initial dimensions, and terminal event/output bounds.

Filesystem, network, environment, and secret rule sets are bounded, canonical, and deny dominant.
Unknown or unrepresentable platform semantics are unsupported rather than widened.

### R-C2-006 — checked sandbox compilation

`compile_sandbox` compares `SandboxRequirements` with the complete contract, rejects undeclared
requests, and returns a private-field `CheckedSandboxPlan`. The plan exposes immutable accessors,
canonical bytes, digest, binding, required enforcement features, operation class, and the exact
admitted capability projections. It remains inert data and does not authorize an effect.

The verified compiler proves the plan is no broader than the contract. Input order does not change
canonical bytes. Changing any authority-relevant field changes bytes and, absent a hash collision,
the plan digest. Secret values and backend-local temporary paths never enter the plan.

### R-C2-007 — fail-closed backend admission

`BackendDescriptor` binds stable backend name/version, native/reference classification, path
semantics, secret delivery modes, resource fidelity, and a complete canonical feature set. Its
digest is part of `ExecutionPlan`.

`admit_backend` requires every feature in the checked plan. It returns a bounded canonical missing
feature list when unsupported. It never removes a requirement, chooses unrestricted execution, or
changes the plan. Restricted execution cannot use a reference-only descriptor in a production
native profile. `ExplicitRawEffect` is a distinct plan and operation class, never a fallback.

### R-C2-008 — executable reference backend

`ReferenceBackend` runs deterministic full support admission and returns a digest-bound
`ReferenceSession`. The session implements `Planned -> Prepared -> Active -> Cancelling ->
Terminated -> Released`, evaluates typed filesystem/program/environment/network/secret/terminal
probes, accounts resource deltas, applies deny/unsupported/violation outcomes, emits bounded ordered
observations, handles pre-activation and active cancellation idempotently, and reports complete or
incomplete teardown.

Named deterministic faults can be injected at support, preparation, activation, observation,
cancellation, termination, and release. Rejected transitions preserve the prior state. The backend
does not spawn an OS process and never claims native containment; it is executable reference
semantics used by C2 and later C3 conformance.

### R-C2-009 — real pipe and PTY execution

The local C2 launcher executes real processes. Pipe mode provides separate bounded stdin, stdout,
and stderr. PTY mode provides one combined terminal stream, bounded input, close, and permitted
resize. Both modes use the same checked command/environment/cwd and produce the same lifecycle and
terminal vocabulary.

Pipe roots are placed in a process group/session on Unix and a job object on Windows through an
audited dependency boundary. PTY launch uses a controlling session where supported and must report
whether complete tree containment is available. A local platform lacking required PTY containment
returns `Unsupported` before launch; C3 provides the native implementation for that platform.

### R-C2-010 — owned lifecycle and tasks

`OwnedProcess` is move-only. The supervisor owns the root child, process-tree controller, stdin,
stdout/stderr or PTY reader, wait observer, cancellation/deadline state, backend preparation/session,
spool, and every support thread/task. No task is detached. `ProcessControl` is a bounded control
handle whose actions are observed by the owner.

The verified lifecycle is:

```text
Authorized -> Starting -> Running -> Stopping -> Exited -> Closed -> Terminal
```

Spawn failure transitions `Starting` directly to a terminal failed record. `Exited` is not
publishable until the owned tree is quiescent, input is closed, output reaches EOF or an explicit
incomplete observation, backend teardown finishes, and support tasks join. Exactly one terminal
record is accepted.

### R-C2-011 — ordered bounded events and output

Every event has the process ID, execution-plan digest, monotonic nonzero sequence, monotonic stream
offset where applicable, kind, and bounded data. Event kinds cover intent persisted, spawn attempt,
started, stdout, stderr, terminal data, stdin accepted/closed, resize, cancellation, escalation,
resource sample/limit, sandbox observation, OS exit, tree quiescence, output completeness, artifact,
and terminal publication.

`OutputPolicy` independently bounds chunk bytes, in-memory retained window, durable spool bytes,
event count, total stdout, stderr, and terminal bytes, and stdin writes. Accounting uses checked
arithmetic. On a ceiling, the supervisor records the exact retained/observed/dropped counts,
classifies completeness, and applies the configured fail/terminate behavior; it never silently
truncates while claiming complete output.

### R-C2-012 — deadlines, cancellation, and escalation

Cancellation reasons are closed: user, deadline, output limit, resource limit, lease fence,
supervisor shutdown, and backend failure. Requests are idempotent. The verified reducer records the
first accepted trigger sequence, which determines the top-level terminal classification even when
exit, cancellation, and deadline observations race.

The supervisor first applies the configured graceful action, waits a bounded grace interval, then
forces the complete owned tree and waits a bounded reap interval. Escalation and cleanup
completeness are recorded. Timeout or cancellation can never become normal success merely because
the child subsequently exits zero.

### R-C2-013 — deterministic terminal and resource observation

`TerminalResult` contains one `TerminalDisposition`, underlying `OsExitObservation`, first trigger,
escalation record, start/end observations, output accounting and artifact references, resource
observations, sandbox observations, tree cleanup, and recovery class.

Dispositions distinguish exited, signalled/platform exception, spawn failed, cancelled, timed out,
output limit, resource limit, sandbox denied, supervisor failed, and recovery indeterminate.
Resource observations distinguish enforced, sampled, unsupported, and incomplete dimensions. A
missing hard enforcement required by the sandbox plan is an unsupported launch, not a successful
best effort.

### R-C2-014 — durable execution registry and restart reconciliation

The process store is rooted outside the agent-visible workspace. Each action/process pair has one
versioned canonical manifest and append-safe lifecycle records. Atomic replacement and directory
sync preserve prepared, starting, running, stopping, and terminal phases. Records bind exact plan,
backend/preparation, process-tree identity, authority consumption, owner, trigger, exit, output,
and terminal digests.

Reopen validates schema, canonical bytes, checksum, namespace, identities, and state progression.
An injected `ProcessProbe` classifies the backend identity as exact live, exact absent, mismatched,
or unverifiable. Results are already terminal, live-owned, absent-unobserved, or indeterminate.
Only an exact live owned tree can be signalled during recovery. Absence without a committed
terminal observation remains non-success. Corrupt or mismatched records are quarantined.

### R-C2-015 — holder-quiescence refinement

The durable ownership registry indexes every process and support task by exact owner and optional
lease claim. `inspect_holder_quiescence` requires the exact reconciliation scope, fenced generation,
prior actor/session holder, and a complete registry scan. It succeeds only when no matching process
or task is starting, running, stopping, cancelling, unresolved, or indeterminate; every started
tree has a terminal observation; every support task joined; and process-tree cleanup is complete.

Only this successful private construction yields `HolderQuiescenceObservation`, which can project
the raw B1 `HolderQuiescenceEvidence` while retaining C2 provenance. A caller-created B1 value is
not sufficient. This rule discharges `REF-C2-B1-HOLDER-QUIESCENCE`.

### R-C2-016 — artifact integration

Live streams first enter a bounded C2-owned spool because C0 artifact writers require the final
size and digest. After terminal closure, C2 re-reads the bounded spool, computes exact digest/size,
and finalizes it through `peritus-artifact-store`. Artifact references identify the exact process,
stream, byte range, completeness, and terminal result.

Artifact publication failure does not rewrite a completed child exit into success or erase the
spool. It returns a terminal result with publication failure and an explicit retry/recovery route.

### R-C2-017 — typed stable failures

`SandboxError` and `ProcessError` expose stable subsystem codes, operation context, recovery class,
and bounded source detail. Expected categories include invalid command/cwd/environment, invalid
contract, plan mismatch, unsupported feature/backend/platform, sandbox denial, stale authority,
receipt reuse, spawn failure, PTY failure, input/output failure, deadline, cancellation, resource
limit, process-tree failure, persistence failure, corrupt recovery record, artifact failure, and
indeterminate recovery. Malformed input and normal environmental failures never panic.

### R-C2-018 — maintainability and formal coverage

Both crates use composition-only roots, responsibility-based modules, private privileged fields,
typed public APIs, no reachable placeholders, and no project unsafe code unless a separately
reviewed narrow platform module with a safety contract becomes unavoidable. Deterministic decision
functions are verified and registered. External process/PTY dependencies remain effect boundaries,
not authority sources.

## Acceptance criteria

1. `peritus-process` and `peritus-sandbox` exist under `crates/runtime`, are owner C2, layer
   `runtime`, verification class H, build in ordinary Rust and Cargo Verus, and satisfy layout/docs.
2. Literal-argv tests prove spaces, quotes, wildcards, variables, redirects, separators, and
   substitutions remain literal and no shell is invoked.
3. Real pipe tests cover exact cwd/environment, bounded stdin/close/broken pipe, separately ordered
   stdout/stderr, zero/nonzero exit, signal where supported, and spawn failure.
4. Real PTY tests cover launch, combined stream, input, EOF, resize, exit, and fail-closed
   unsupported containment on platforms where the C2 local launcher cannot provide it.
5. Process-tree tests start a child and grandchild, cancel or time out the root, prove the owned
   tree is terminated/reaped, and prove no supervisor/output task remains live.
6. Output tests cover every exact bound and one-over path, monotonic offsets/sequences, bounded
   memory/spool/events, completeness/truncation, and artifact finalization/retry behavior.
7. Lifecycle tests cover every legal/illegal state edge, first-trigger precedence, exactly one
   terminal result, deadline/cancel/exit races, graceful and forced escalation, and no implicit
   success.
8. Resource tests cover wall, output, process count, sampled CPU/memory where supported, hard-limit
   unsupported behavior, and exact resource observation accounting.
9. Recovery tests reopen every durable phase, classify terminal/exact-live/absent/mismatched/
   unverifiable states, reject corrupt or cross-process manifests, and never infer success.
10. Authorization tests independently drift action, plan/payload/media, owner lineage, actor, role,
    environment, resource, capability, budget, operation class, dispatch, every revision field,
    generation, holder/session, lease, epoch/tick/expiry, sandbox digest, backend fingerprint, and
    preparation digest; every rejection produces zero effects.
11. Compile-fail/API tests prove callers cannot name or construct the execution permit or authorized
    launch and cannot invoke the local launcher without the gateway.
12. Durable consumption tests prove a process/action authorization cannot be replayed after success,
    spawn failure, process restart, or daemon restart.
13. Holder-quiescence tests cover direct child, descendant, starting, cancelling, unresolved
    restart, wrong holder/claim/generation, unrelated holder, terminal cleanup, and complete task
    joins. Only the exact complete final case produces evidence.
14. Sandbox tests cover constructors/bounds, default denial, allow/deny dominance, environment,
    network, secret references, process descendants, resources, PTY controls, canonical
    determinism, field drift, backend admission, unsupported no-effect, observations, cancellation,
    faults, and lifecycle.
15. Secret canaries prove raw secret bytes never appear in plan bytes/digests, debug output, errors,
    or enforcement observations.
16. Fresh-subject A2 process and sandbox conformance suites are populated with real cases and pass
    against the C2 production/local and reference adapters. C3 can reuse the sandbox suite without
    depending on C2 implementation internals.
17. Named Verus rules and executable tests register `INV-013` and C2 obligations for process
    ownership, authority completeness, holder quiescence, terminal/output accounting, and sandbox
    contract/backend refinement.
18. `REF-C2-B1-AUTHORITY-GATE` and `REF-C2-B1-HOLDER-QUIESCENCE` are removed only after exact proof
    symbols and tests exist. No other reservation is removed.
19. Cargo, lockfile, architecture, dependency policy, strict Verus/no-cheating lists, proof
    obligations, CI, conformance catalogs, and documentation register both crates.
20. Focused tests, format, strict Clippy/rustdoc, architecture/source/trust/API/reproducibility
    checks, full Verus verification/build, `just check`, and local `just gate-a` pass.
21. Bounded cross-review finds no unresolved correctness, authority, lifecycle, recovery, or
    maintainability defect.
22. The final change is signed, pushed, merged through the protected-main pull-request path, exact
    `origin/main` contains the signed source commit, repository protections are restored, and
    Crosslink issue #11/session are closed.

## Current architecture

B0 exposes the exact current dispatched `ActionState` and its checked B1 authorization witness only
inside C0's opaque `CommittedKernelTransition`. B1 capability transitions bind action, digest,
actor, role, environment, permission, revision, validity, and time floor. B1 budget transitions bind
reservation, action, digest, revision, and active-effect ceilings. B1 lease uses bind exact
workspace/resource/environment, holder, generation/version, capability, and expiry. C0 returns
separate move-only committed receipts and a non-forgeable current authority epoch. B3 supplies
canonical `ActionIntentDto` bytes and digests.

C1 demonstrates the required target-owned gateway, durable one-use marker, exact dispatch-frame
decode, Verus comparison predicates, refinement proofs, and no-effect drift tests. C1 deliberately
does not claim holder quiescence. A2 has an intentionally empty sandbox catalog and no process suite.
There is no general process or sandbox runtime crate.

No B0/B1/B3/C0 public API change is required for C2. C2 consumes their frozen APIs.

## Proposed design

### Crate and dependency boundaries

```text
peritus-sandbox -> peritus-types, peritus-codec, vstd

peritus-process -> peritus-sandbox, peritus-types, peritus-codec, peritus-protocol,
                   peritus-kernel, peritus-policy, peritus-budget, peritus-leases,
                   peritus-journal, peritus-artifact-store, vstd,
                   process-wrap, portable-pty, nix (Unix only)
```

`peritus-process -> peritus-sandbox` is the only direction between C2 crates. Native C3 backends
depend on both and implement process-owned launch/backend interfaces. `peritus-sandbox` never
depends on process or exposes a public OS spawn.

Pinned external candidates are `process-wrap = 9.1.0` with only the std/process-group/session/
job-object/creation-flags features required for owned pipe execution, and `portable-pty = 0.9.0`
without Serde features. Unix PTY tree cancellation uses the safe `nix = 0.31.3` `killpg` surface
against the session leader exposed by `portable-pty`; `process-wrap` cannot adopt an already
spawned PTY child. Their unsafe platform work remains dependency code. Exact dependency and
duplicate-version policy changes are accepted only after the resolved Cargo graph and `cargo deny`
prove the concrete need.

### `peritus-sandbox` modules

```text
src/lib.rs
src/error.rs
src/binding.rs
src/feature.rs
src/filesystem.rs
src/process_policy.rs
src/environment.rs
src/network.rs
src/secret.rs
src/resource.rs
src/terminal.rs
src/contract.rs
src/requirements.rs
src/canonical.rs
src/plan.rs
src/backend.rs
src/admission.rs
src/observation.rs
src/cancellation.rs
src/lifecycle.rs
src/reference.rs
src/reference/session.rs
src/reference/evaluation.rs
src/reference/accounting.rs
src/verified.rs
src/refinement.rs
```

### `peritus-process` modules

```text
src/lib.rs
src/error.rs
src/identity.rs
src/command.rs
src/environment.rs
src/working_directory.rs
src/io_policy.rs
src/resource.rs
src/plan.rs
src/intent.rs
src/authorization.rs
src/gateway.rs
src/consumption.rs
src/lifecycle.rs
src/events.rs
src/terminal.rs
src/cancellation.rs
src/control.rs
src/supervisor.rs
src/quiescence.rs
src/output.rs
src/output/spool.rs
src/output/window.rs
src/recovery.rs
src/recovery/manifest.rs
src/recovery/reconcile.rs
src/platform.rs
src/platform/pipe.rs
src/platform/pty.rs
src/platform/ownership.rs
src/verified.rs
src/refinement.rs
```

`lib.rs` files contain crate documentation, module declarations, and intentional re-exports only.
Files split before the 700-line hard source limit and receive review at the 400-line soft budget.

### Public effect boundary

```text
SandboxContract + Requirements
        -> verified CheckedSandboxPlan
        -> fail-closed backend admission/preparation
Structured ExecutionPlan + committed B0/B1/C0 observations
        -> verified exact target authorization
        -> durable one-use consumption + execution intent
        -> private ExecutionPermit
        -> opaque AuthorizedLaunch
        -> owned local/native launcher
        -> bounded observations
        -> verified lifecycle/output/resource reduction
        -> durable TerminalResult + artifacts
```

The public launcher trait accepts only `AuthorizedLaunch`, whose constructor and fields are private
to `peritus-process`. A C3 implementation can inspect safe exact accessors needed to launch but
cannot synthesize the value. The C2 local launcher is held privately by `ExecutionGateway` and is
covered by the same interface.

### Workspace access and lease rule

`WorkspaceAccess::Writable` requires the exact committed lease use and all C1-equivalent holder,
scope, generation, version, capability, and time checks. `WorkspaceAccess::ReadOnly` requires a
read-only target identity and rejects a supplied mutation lease as surplus authority. This supports
both writer/fixer shell work and read-only gate/reviewer execution without weakening exclusive
mutation leases.

### Reference versus native enforcement

The reference backend is marked `ReferenceOnly` and never satisfies a production `Restricted`
native launch requirement. It runs the full semantic conformance surface in memory. The C2 local
process launcher may run only plans whose backend descriptor truthfully reports the controls it
provides. If a required native control is absent, launch is unsupported. C3 adds the tier-one native
descriptors and preparations without changing C2 values or policy.

### Formal model and proof ownership

The following entries are added to `verification/obligations.toml` with issue `#11`, registered
actor, exact symbols, commands, and real tests:

- `INV-013`, `peritus-process`: every live execution has one exact owner and at most one accepted
  terminal disposition; fair-supervisor eventual completion is conformance/resilience evidence.
- `OBL-0126`, `peritus-process`: the private permit implies complete committed authority and exact
  execution/sandbox/backend plan binding.
- `OBL-0127`, `peritus-process`: holder-quiescence evidence implies exact claim correlation,
  complete inspection, zero live owned processes/tasks, and complete tree cleanup.
- `OBL-0128`, `peritus-sandbox`: checked plans are no broader than contracts and backend admission
  covers every required feature.
- `OBL-0129`, `peritus-process`: lifecycle/output accounting yields monotonic events, bounded
  retained bytes, first-trigger terminal classification, and at most one terminal result.

Exact final dependency IDs are checked against the existing manifest rather than guessed. Entries
remain `in-progress` under the repository's independent-review status convention.

### A2 conformance

A2 adds runtime-neutral contracts under:

```text
crates/app/testing/peritus-conformance/src/process.rs
crates/app/testing/peritus-conformance/src/process/cases.rs
crates/app/testing/peritus-conformance/src/sandbox.rs
crates/app/testing/peritus-conformance/src/sandbox/cases.rs
```

The A2 crate does not depend on either runtime crate. Subject traits translate A2 fixtures into the
production type system. Cases use `SubjectFactory` so each starts with a fresh subject.

Process conformance covers literal argv, cwd/environment, pipe/PTY streaming, output bounds,
cancellation/deadline, process-tree cleanup, terminal uniqueness, restart classification, and
authorization no-effect behavior. Sandbox conformance covers default denial, filesystem,
environment/secrets, network, descendants/PTY, exact resource boundary, unsupported no-effect,
cancellation/teardown, observation binding, and canonical preparation reproducibility.

### Parallel implementation ownership

After this design freezes the boundary:

| Lane | Exclusive paths | Prohibited overlap |
|---|---|---|
| Sandbox | `crates/runtime/peritus-sandbox/**` | process, A2, root/shared files |
| Process | `crates/runtime/peritus-process/**` | sandbox, A2, root/shared files |
| Conformance | C2 files under `crates/app/testing/peritus-conformance/**` | runtime crates, root/shared files |
| Integration | design, root Cargo/lock, architecture, verification, CI, Justfile, docs, xtask | runtime internals except coordinated review fixes |

The integration owner freezes shared signatures, performs Cargo/lock registration once, and owns
all collision-prone lists. Runtime lanes use crate-local tests and fixtures until A2 adapters land.

## Data and compatibility

Execution, sandbox, backend descriptor, preparation, output spool, and recovery manifests begin at
schema version one. Exact field order, discriminants, bounds, domain separators, and digest inputs
become compatibility-sensitive once C4/D0 persist or transport them. Later incompatible changes
add versions and retain decoders/migration fixtures; they do not reinterpret old bytes.

The process registry is implementation recovery data under the protected Peritus runtime root.
C0 remains the authoritative domain journal. C2 manifests prove what the process owner observed and
support reconciliation; they do not independently authorize lifecycle acceptance. Terminal/output
artifacts are content addressed through C0.

No production C2 records exist, so C2 needs no C0 database migration. Existing C0 schemas and B3
families remain unchanged.

## Failure handling

- Invalid command, environment, cwd, contract, requirement, resource, and terminal inputs fail
  before authorization or effect.
- Unsupported backend controls fail before durable action consumption or launch.
- Authority mismatch fails before sandbox activation, spool creation, or child creation.
- Failure after durable consumption but before spawn records a terminal spawn/preparation failure;
  it never makes the same action reusable.
- Failure after spawn retains ownership and drives cancellation/reap. Dropping a handle requests
  bounded shutdown and cannot claim completion without an observation.
- Output failure or overflow records exact completeness and applies policy; it does not silently
  discard data while reporting complete output.
- Deadline/cancel/resource/backend races use first accepted trigger sequence and preserve the OS
  exit separately.
- Backend teardown or tree cleanup failure yields indeterminate/reconciliation-required, never
  successful quiescence.
- Restart signals only an exact matched live tree. PID reuse, missing start identity, corrupt
  manifest, or incomplete probe blocks signalling and quiescence.
- Artifact finalization failure retains the spool and recovery route; process exit facts remain
  unchanged but publication is incomplete.
- Every error provides a stable code and recovery class: caller correction, reauthorize, retry
  preparation, cancel/reap, reopen/reconcile, quarantine, retry publication, or terminal.

## Security considerations

Model-provided strings remain data and never enter a shell. Authorization and display derive from
the same canonical plan. Child environments start cleared, secret values never enter canonical
bytes or ordinary observations, and sandbox backend absence is not unrestricted fallback.

The action intent, plan, sandbox contract, selected backend support, and preparation are all digest
bound. Capability and lease checks are repeated immediately before effect. Move-only private permits
and durable consumption prevent public or replay bypass. Backend adapters cannot declare a run
accepted or construct B0/B1/C0 receipts.

C2 implements normal production paths and concrete failure handling. Escape-focused native sandbox
testing, host-specific path alias/handle rules, DNS rebinding, secret stores, and OS hard-resource
mechanisms are C3/H0/H2 responsibilities exercised against C2's complete contract. C2 does not
invent speculative platform controls it cannot enforce, and it never misreports their absence.

## Verification

Focused development commands are:

```text
cargo test --package peritus-sandbox --package peritus-process --package peritus-conformance --all-targets --all-features --locked
cargo clippy --package peritus-sandbox --package peritus-process --package peritus-conformance --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-sandbox --package peritus-process --package peritus-conformance --all-features --no-deps --locked
cargo verus verify --package peritus-sandbox --package peritus-process --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo run --locked --package xtask -- all
just check
just gate-a
```

Real subprocess tests use the current test executable as a platform-neutral helper rather than a
shell. PTY tests use a deterministic helper command and platform capability assertion. Time-sensitive
tests use bounded generous deadlines and event-driven synchronization, not arbitrary sleeps.
Fault tests inject returns at named durable/effect boundaries.

## Rollout and rollback

C2 lands as both libraries, A2 cases, manifests, documentation, and verification evidence in one
protected-main change. This is an internal implementation stage, not an MVP or release. C4 and D0
consume the frozen process surface only after C2 merges.

Before downstream persisted consumers land, rollback removes C2 registrations and crates. After
consumers exist, old execution/sandbox/recovery schemas remain readable and incompatible behavior
requires a forward version. Removing native backend support later is an explicit unsupported
capability and cannot silently choose raw execution.

Delivery completes only after focused checks, full local Gate A, bounded concrete cross-review,
cause-level fixes, a signed source commit, feature push, protected pull request, merge to main,
signed-commit ancestry verification, protection restoration, and Crosslink closure.

## Open questions

None block implementation. The sole effect gateway, one-way dependency, optional lease rule by
workspace access, reference/native distinction, canonical plan identity, and C2/C3 boundary are
frozen by this design.

## Out of scope

- Native Linux namespaces/Landlock/seccomp/cgroups, macOS Seatbelt, Windows restricted-token/
  AppContainer/job integration beyond the C2 local process owner, managed network enforcement, and
  secret-store injection (C3).
- Model-facing shell scripts and built-in shell/fs/Git/quality tools (C4). C2 exposes structured
  argv; C4 may define a separately authorized shell-script tool through it.
- Gate planning/evidence interpretation (D1), scheduler cancellation trees (D3), and complete
  writer/reviewer/fixer orchestration (E0).
- Daemon-wide startup epoch fencing and intake sequencing (G0). C2 supplies process recovery and
  quiescence primitives consumed there.
- Tier-one native escape qualification, cross-platform parity verdicts, soak/load SLOs, and release
  qualification (H0/H2/H3/H4).

These are ownership boundaries, not optional capabilities or reduced quality. C2 freezes and tests
the complete interfaces those slices must satisfy.

## Alternatives considered

### Sandbox-owned spawn and authorization

This contradicts `REF-C2-B1-AUTHORITY-GATE`, duplicates authority, and creates pressure for a
process/sandbox dependency cycle. Rejected. `peritus-process` owns authorization and effect;
`peritus-sandbox` owns checked inert plans and preparation semantics.

### Public backend `spawn(command, policy)`

This would let callers bypass committed authority while relying on convention. Rejected. Launchers
accept only an opaque process-owned `AuthorizedLaunch`.

### Shell command strings

They make authorization bytes differ from executed parsing and create platform-dependent injection
semantics. Rejected. C2 accepts structured executable and argv only.

### Unbounded in-memory output

This makes output-heavy tools a daemon memory failure and cannot express completeness. Rejected.
Streaming windows, bounded spools, exact counters, backpressure, and artifacts are required.

### Treat missing controls as best effort

This silently widens authority and makes conformance meaningless. Rejected. Required feature
admission is complete and fail closed; explicit raw effect is a separately authorized plan.

### Implement all native sandbox backends inside C2

This would erase canonical crate ownership, collide platform teams, and couple the abstract plan to
three OS implementations. Rejected. C2 is complete at the platform-neutral and real local process
backplane boundary; C3 supplies independent native implementations of that frozen contract.

## Architecture verdict

`ready`

The design supplies the complete production C2 behavior, exact authority and failure boundaries,
stable public contracts, durable recovery, formal obligations, reusable conformance, and
collision-free implementation lanes. No product or architecture decision remains unresolved.
