# peritus-conformance

`peritus-conformance` is Peritus's runtime-neutral conformance harness. It supplies typed suite,
case, subject-factory, failure, observation, and report contracts without defining any production
provider, tool, journal, process, sandbox, plugin, protocol, or replay implementation.

## Invariants

- Case execution order is the bytewise order of validated case identifiers, independent of case
  registration order.
- Duplicate case identifiers invalidate a suite before any subject is created.
- Every run polled to `Ready` gives each executed case a fresh subject. A subject returned by a
  setup future whose completed future is destroyed without panic is passed to teardown exactly
  once, including after an assertion failure or caught case panic.
- Assertion, setup/execution, and teardown failures remain typed and distinct. No failure or panic
  is converted into success.
- Every human-readable value admitted to a report uses validated nonempty `ReportText` of at most
  4096 UTF-8 bytes. Oversized caller values are rejected without truncation; an oversized panic
  message is replaced by an explicit omission diagnostic retaining its original byte length.
- An empty suite is runnable and reports `Empty`; only a nonempty `Passed` report proves
  conformance.
- Reports contain no clocks, durations, thread identifiers, addresses, or backtraces introduced by
  the runner. Case-supplied observations retain their explicit order.
- Typed observations support Boolean, signed, unsigned, bounded text, and exact 32-byte digest
  values. Digest observations do not claim hashing or authenticity semantics.

Failure analysis derives `ContractViolation` only from a returned assertion and `Infrastructure`
from setup, caught panic, or teardown failure. Suite summaries count affected cases rather than
failure occurrences. A case with an assertion followed by teardown failure contributes once to
both category counts.

## Panic containment

The runner catches ordinary Rust unwinding around suite metadata, subject metadata, case metadata,
future construction, future polling, completed-future destruction, and teardown. Panic messages
are bounded or explicitly omitted before entering reports. In a run polled to `Ready`, catching a
case panic does not skip teardown of its successfully established subject.

The runner future remains ordinarily cancellable. Dropping it while an operation is pending drops
that in-flight future and any owned subject in place. Cancellation before teardown begins does not
call asynchronous teardown; cancellation of an already-pending teardown future drops it without
awaiting completion. A panic from a pending future's destructor unwinds from the caller's drop
operation and cannot enter a report that will never be produced. Subject types must therefore
provide cancellation-safe synchronous RAII cleanup. Production supervisors must poll
qualification runs to a terminal report or classify external cancellation as an infrastructure
failure.

In-process unwinding is not process isolation. `panic = "abort"`, explicit aborts, out-of-memory
termination, stack overflow, undefined behavior, foreign-code termination, a panic while already
unwinding, and operating-system process death cannot be converted into a report. Callers that need
evidence for those failures must execute conformance in a supervised subprocess.

## Catalog suites and ownership

The crate exports production journal, replay, C1 workspace, C2 process, C2/C3 sandbox, C4 tool,
C5 provider, D0 agent-loop, D1 gate, D2 review, D3 scheduler/collaboration, E0 orchestrator, E1
harness materialization, E2 debugger, and C7 trace/telemetry suites plus runnable empty plugin and
protocol suites. The provider suite contains
fourteen fixed cases covering capability honesty, ordering and exact deduplication, fragmented
tool calls, malformed and incomplete streams, interruption, cancellation, authentication, rate
limits and retry-after, transient retry, ambiguous submission, usage, redaction, and adapter
isolation. Its subjects return direct event, attempt, usage, and routing observations rather than
self-reported verdicts. The tool suite exercises
descriptor/schema determinism, schema rejection before effect, canonical role/capability exposure,
exact one-use dispatch, independent authority drift, truthful structured results, owned controls
and deadlines, and replay without duplicate effects. The process suite exercises
literal structured invocation, pipe and PTY observations, bounded output, cancellation, deadlines,
tree ownership, terminal uniqueness, restart classification, and authorization no-effect. The
sandbox suite exercises default denial across every domain, deny dominance, exact environment,
secret, network, descendant, terminal, and resource policy, unsupported admission, cancellation,
observation binding, and deterministic inert preparation. Every case receives a fresh subject.
The agent suite contains thirteen fixed cases covering the complete inspect/edit/run/test cycle,
pause/resume, cancellation, prefix replay, context composition, provider reduction, safe retry,
independent tool authorization, active controls, parallel result ordering, budget exhaustion,
completion eligibility, and crash recovery without uncertain-effect redispatch. Its subjects report
direct transition, ownership, ordering, authority, revision, and completion observations.
The gate suite contains ten fixed cases covering the complete inspect/edit/run/test path, failed
prerequisites, malformed parser output, stale revision, cancellation, crash recovery, clean
snapshots, bounded retries, artifact evidence, and deterministic aggregation. The trace suite
contains nine fixed cases covering causal integrity, redaction leakage, bounded load, exporter
failure, durable replay, duplicate conflict, backpressure, shutdown recovery, and non-authority.
The review suite contains ten fixed cases covering lifecycle, quorum, every independence
dimension, duplicate reconciliation, stale revisions, reviewer-confirmed resolution, externally
authorized waivers, restart, oscillation, and malformed submissions. Their negative tests
deliberately inject implicit gate/review success and default-surface canaries and require the
corresponding suites to fail.

The scheduler and collaboration suites cover deterministic resource conservation, worker/task
ownership, causal joins, cancellation, and restart. The orchestrator suite covers the complete
writer/gate/reviewer/fixer order and durable B0 acceptance observation. The harness suite contains
fourteen cases covering exact manifest inventory, the complete component catalog, graph and
authority rejection, protected immutability, content-addressed history, forward and rollback C1
materialization, finalized artifacts, independent bounds, replay/idempotency, malformed frames,
panic containment, and teardown isolation.
The debugger suite contains thirteen cases covering immutable evidence selection, canonical
timelines, closed taxonomy, citation containment, invalid model-output rejection, deterministic
clustering, replay, durable cancellation, malformed input, redaction, independent bounds, panic
containment, and teardown isolation.

Production crates implement only the subject traits and translate these runtime-neutral fixtures
and observations into their own domain values. A2 neither constructs privileged runtime permits
nor depends on a process or sandbox implementation. An empty suite's `Empty` status is scaffolding,
not Gate evidence. Later protocol-owning slices add typed cases after their contracts exist.

This crate deliberately does not define model messages, provider wire formats, production
streaming normalization, production tool schemas, authorization receipts, retry policy,
idempotency storage, journal records, or production error taxonomies. Those remain owned by B3 and
the relevant C slices; A2 exposes only runtime-neutral fixtures and direct observations for
qualifying C4 tools and C5 provider adapters.

## Dependency policy

This verification-class `C` crate is std-only and belongs to the `testing` layer. Its boxed futures
do not select or embed an async runtime. Callers poll the returned runner future with their chosen
executor.
