# C2 process and sandbox backplane

C2 is Peritus's production process-ownership boundary. It turns an already-dispatched action,
durably committed B1 authority, and a complete checked sandbox plan into one owned execution. It
does not decide what work should run, grant authority, or interpret a successful tool result as an
accepted run.

The slice consists of two verification-class `H` runtime crates:

| Crate | Responsibility |
|---|---|
| `peritus-process` | Structured commands, exact launch authorization, process ownership, bounded I/O, lifecycle, terminal accounting, durable recovery, and holder quiescence |
| `peritus-sandbox` | Complete platform-neutral sandbox contracts, checked plans, backend admission, observations, cancellation, and executable reference semantics |

`peritus-process` depends on `peritus-sandbox`; the reverse dependency is prohibited. Native C3
backends implement the interfaces frozen by C2. They may report that a required control is
unsupported, but they cannot weaken or reinterpret a checked plan.

## Structured execution

Commands contain one executable and an ordered argument vector. They are never parsed by a shell.
The working directory is checked before authorization and remains bound to the workspace,
resource, environment, generation, and revision selected for the execution. Environment handling
starts from a cleared child environment and adds only the checked allowlisted or explicit entries.
Inherited and literal bindings retain distinct provenance through plan validation and canonical
identity, so authority to inherit a name cannot be repurposed to inject a caller-selected value.

An execution identity carries the complete project/session/run/attempt/turn/action/process lineage
plus workspace, resource, environment, and revision identities. The execution plan additionally
binds:

- command, working directory, and deterministic environment;
- separate pipes or PTY mode and terminal controls;
- input, output, deadline, escalation, and resource policies;
- read-only or writable workspace access;
- the complete checked sandbox-plan digest; and
- the selected backend's descriptor, support, and preparation identity.

Canonical versioned bytes cover every authority-relevant field. Their digest is also carried by
the B3 execution intent, so the bytes authorized by B0/B1/C0 are the plan consumed by the process
owner.

## Sandbox contract

`peritus-sandbox` represents all seven capability domains instead of treating a sandbox as a
single enabled flag:

1. filesystem discovery, metadata, read, execute, create, write, and removal;
2. root execution, descendant creation, signals, containment, and process counts;
3. cleared or allowlisted environment and declared delivery destinations;
4. outbound network rules with inbound access denied;
5. secret references and delivery channels without embedding secret values;
6. wall, CPU, memory, disk, output, descriptor, process, and concurrency limits; and
7. pipes, PTY, input, resize, signal, dimension, event, and terminal-output controls.

Contracts and requirements are bounded and canonical. Compilation rejects undeclared authority
and produces inert checked data; it does not authorize execution. Backend admission compares the
entire required feature set with a canonical backend descriptor. Missing enforcement returns a
typed unsupported result before launch. Restricted execution never falls back to raw execution.

The executable reference backend implements the full platform-neutral lifecycle, probe,
observation, accounting, cancellation, fault, termination, and release semantics. It deliberately
does not spawn or claim native host containment. This makes it a deterministic conformance oracle
for C2 and the later native C3 implementations.

## Authorization boundary

`ExecutionGateway` is the only product-facing process effect boundary. Callers supply an
unprivileged authorization request containing the checked intent and the exact move-only C0
observations produced after committing:

- the B0 action dispatch;
- the B1 capability use;
- the B1 budget begin transition; and
- for writable workspace execution, the B1 active lease use.

The request also supplies C0's current authority epoch, the exact current time observation,
session, revision, expected generation/revision, and execution plan. Immediately before any
sandbox activation or process creation, the gateway cross-checks the complete owner lineage,
action identity and digest, actor and role, operation class, environment, resource, capability,
budget, conditional lease, workspace access, revision tuple, authority time, sandbox digest,
backend support, preparation, and plan digest.

Writable execution requires the exact committed mutation lease. Read-only execution requires a
read-only target and rejects surplus mutation-lease authority. A mismatch or stale observation
returns a typed error with no process effect.

After successful comparison, the gateway durably consumes the action/process pair and immediately
uses its crate-private permit to create the authorized launch value. Neither the permit nor a raw
spawn surface is public. A failure after durable consumption is recorded and recovered; it does
not make the authority reusable.

## Process ownership and I/O

Pipe execution keeps stdin, stdout, and stderr distinct. PTY execution exposes one terminal stream
with checked input, close, and permitted resize. Both use the same command, environment, working
directory, lifecycle, event, output, cancellation, and terminal-result vocabulary.

Every root process and descendant belongs to one execution identity. The supervisor owns the
child, process-tree controller, input endpoint, output readers, wait observer, backend session,
durable spool, cancellation/deadline state, and support tasks. Support work is joined before an
execution is declared closed; a detached task is not treated as completion.

Events are monotonically sequenced and bind the process and plan digest. Output policies bound
each chunk, retained memory, durable spool, event count, stream totals, and stdin writes. When a
limit is reached, accounting records observed, retained, and dropped bytes and applies the checked
policy. Truncation is never reported as complete output.

Completed bounded spools can be finalized through C0's artifact store. Artifact publication is
separate from the OS exit observation: a zero exit does not turn incomplete output publication
into complete evidence. Per-stream publication progress is durable and idempotent; an artifact
failure returns the completed terminal result with retry guidance, and restart retries only missing
streams. A process with no retained output completes publication without inventing an artifact.

On Linux, the local raw-effect supervisor samples the owned process group for CPU time, resident
memory, process count, open handles, and workspace disk growth at bounded cadences and terminates
observed overruns. Disk scans do not follow symbolic links. Terminal records carry all eight typed
resource dimensions and their actual enforcement fidelity. A platform without complete supervisor
sampling may execute only separately authorized raw-effect/reference plans; a plan claiming native,
hard, or unavailable supervisor enforcement is rejected before durable consumption. C3 supplies
the native cross-platform enforcement backends.

## Lifecycle and termination

The process owner reduces executions through explicit authorized, starting, running, stopping,
exited, closed, and terminal phases. A spawn failure takes the starting path directly to a failed
terminal record. Exactly one terminal result is accepted.

Cancellation triggers distinguish user request, deadline, output or resource limit, lease fence,
supervisor shutdown, and backend failure. The first accepted trigger determines the top-level
classification even if an OS exit races with cancellation. The underlying exit remains separately
observable.

Shutdown first applies the configured graceful control, waits for the bounded grace interval,
then forces and reaps the owned tree within the bounded escalation interval. A terminal result
distinguishes normal exit, platform signal or exception, spawn failure, cancellation, timeout,
output/resource limit, sandbox denial, supervisor failure, and recovery indeterminacy. It also
records output completeness, escalation, resource observations, backend observations, and tree
cleanup.

## Durable recovery

The execution registry lives outside the agent-visible workspace. Versioned records bind the
complete plan, authority consumption, process-tree identity, lifecycle, cancellation trigger,
output accounting, artifact-publication progress, complete terminal observation, and its versioned
digest. Reopen validates schema, canonical bytes, checksum, namespace, identity, and legal phase
progression. A durable one-use claim without its corresponding manifest is retained as an explicit
indeterminate orphan record rather than disappearing from reconciliation or `all_terminal`.

A process probe classifies a reopened record as already terminal, exact live owned tree, absent
without a terminal observation, or indeterminate. Only an exact live owned tree can be controlled
during recovery. PID reuse, mismatched start identity, corrupt records, or unverifiable state do
not permit signalling and never imply success.

Holder-quiescence inspection correlates the exact fenced generation and prior actor/session holder
with every process and support task in the ownership registry. Evidence is produced only after a
complete scan finds no matching live or unresolved execution, every started tree has terminated,
every task has joined, and cleanup is complete. This supplies C2 provenance for B1 resource
reconciliation; a caller-created logical value is not a substitute.

## Stable failures

Expected malformed input and ordinary platform failures return typed errors. Stable error codes
and recovery classes distinguish caller correction, reauthorization, preparation retry,
cancel/reap, reopen/reconcile, quarantine, publication retry, and terminal failure. In particular:

- invalid commands, environments, paths, contracts, and requirements fail before authorization;
- unsupported controls fail before durable consumption and launch;
- authority drift fails before any sandbox or process effect;
- post-consumption spawn failure remains consumed and terminally recorded;
- output or backend failure retains exact incomplete observations;
- cleanup uncertainty blocks holder quiescence; and
- absence after restart without a terminal record remains non-success.

## Conformance and formal evidence

A2 publishes fresh-subject process and sandbox suites independent of both runtime crates. Runtime
adapters translate those cases into production types. Process conformance covers literal argv,
cwd/environment, pipes and PTY, bounds, cancellation, deadlines, cleanup, terminal uniqueness,
restart classification, and authorization rejection with no effect. Sandbox conformance covers
all seven domains, default denial, canonical plans, backend support, unsupported no-effect,
cancellation, teardown, observation binding, and reproducible preparation.

Focused production regressions additionally exercise post-spawn failure cleanup, descendants that
retain output pipes after root exit, denied resize without owner failure, inherited/literal
environment separation, process-count cancellation, symlink-safe disk sampling, artifact retry,
zero-output publication, orphan claims, and complete terminal-digest sensitivity.

The C2 proof obligations cover exact process ownership and terminal uniqueness, complete gateway
authority, holder quiescence, sandbox compilation/backend admission, and monotonic bounded
lifecycle/output accounting. Deterministic policy, comparison, reduction, and classification code
is Verus Rust. Process creation, PTY and process-tree APIs, clocks, resource probes, durable
filesystem operations, and artifact transfer remain narrow effect shells.

## Verification

Focused development checks are:

```text
cargo test --package peritus-sandbox --package peritus-process --package peritus-conformance --all-targets --all-features --locked
cargo clippy --package peritus-sandbox --package peritus-process --package peritus-conformance --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-sandbox --package peritus-process --package peritus-conformance --all-features --no-deps --locked
cargo verus verify --package peritus-sandbox --package peritus-process --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
just gate-a
```

Real-process tests use structured test helpers, never shell command strings. The complete local
Gate A remains the merge authority when hosted GitHub runners cannot start because of the known
account-level runner restriction.
