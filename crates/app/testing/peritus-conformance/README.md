# peritus-conformance

`peritus-conformance` is Peritus's runtime-neutral conformance harness. It supplies typed suite,
case, subject-factory, failure, observation, and report contracts without defining any production
provider, tool, journal, sandbox, plugin, protocol, or replay interface.

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

## Empty catalog suites and future ownership

The crate exports runnable empty provider, tool, plugin, sandbox, journal, protocol, and replay
suites. Their `Empty` status is scaffolding, not Gate C evidence. Later protocol-owning slices add
typed cases after their contracts exist.

This crate deliberately does not define model messages, streaming normalization, tool schemas,
capabilities, authorization, retries, idempotency, journal records, or production error taxonomies.
Those remain owned by B3 and the relevant C slices.

## Dependency policy

This verification-class `C` crate is std-only and belongs to the `testing` layer. Its boxed futures
do not select or embed an async runtime. Callers poll the returned runner future with their chosen
executor.
