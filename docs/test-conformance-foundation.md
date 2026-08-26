# Test and conformance foundation

Slice A2 provides deterministic testing mechanics for every later Peritus subsystem. It consists
of two verification-class `C` crates:

- `peritus-test-support` owns deterministic fixtures and fixture-file validation.
- `peritus-conformance` owns protocol-neutral suite execution and verdict reporting.

Neither crate is an authority boundary. They consume checked A1 values, but they do not define
production clocks, identifiers, events, provider or tool protocols, acceptance policy, or domain
state. Later crates normally use them through development dependencies and retain ownership of
their production interfaces.

## Determinism contract

A fixture must make every input to observable behavior explicit. Tests must not read the wall
clock, draw random identifiers, contact the network, inherit a user's Git configuration, sleep to
coordinate work, or depend on scheduler order. Deliberately malformed data remains exact bytes;
the fixture layer performs no newline, timestamp, path, encoding, or snapshot normalization.

The support crate provides these independent mechanics:

- a clock anchored at an explicit `SystemTime`, with atomic checked advancement of wall and
  monotonic readings;
- a non-cloneable identifier stream whose bytes are exactly an eight-byte namespace followed by a
  big-endian nonzero counter;
- event fixture contexts containing only an `EventId` and `EventSequence`;
- occurrence-indexed fault schedules with stable points and labels, shared clones, independent
  forks, and explicit missed-fault verification;
- FIFO call and stream scripts that preserve duplicate values and domain errors, record mismatches
  without consuming the expected response, and reject exhaustion or incomplete use;
- provider and tool brands around those generic scripts, without provisional production traits;
- repository-relative fixture paths and temporary Git repositories hardened against traversal,
  symlink escape, host configuration, signing, hooks, line-ending conversion, and variable commit
  identity or timestamps; and
- strict, content-addressed fixture manifests and catalogs.

Errors are typed and expose stable codes. A simulated domain failure is an ordinary scripted
outcome; it is not conflated with a script violation or infrastructure failure. Values that own a
sequence are intentionally non-cloneable. Clock and fault-injector clones share state by contract;
their `fork` operations create independent deterministic snapshots.

## Fixture layout

Compatibility evidence uses this canonical repository-relative layout:

```text
compat/<surface>/<surface-version>/<case>/fixture.toml
```

Ordinary non-compatibility data may use:

```text
fixtures/<suite>/<case>/fixture.toml
```

Placement under `fixtures/` does not constitute compatibility evidence. A version string is an
opaque validated path component at this layer; A2 does not impose the protocol-version semantics
owned by A3 or B3.

A compatibility manifest has schema version 1, a validated surface, surface version and case, one
of the fixture kinds `minimal`, `realistic`, `corrupt` or `adversarial`, and a strictly sorted list
of file paths with lowercase SHA-256 digests. The shared envelope intentionally contains no
request, event, response, error or expected-verdict schema.

Loading or verification fails closed on:

- unknown manifest fields or schema versions;
- invalid names or paths;
- duplicate or unsorted paths;
- absolute paths, prefixes, `.` or `..` components;
- symlinked files or directory ancestors;
- missing, extra or unlisted files; and
- digest mismatches.

Released compatibility surfaces must satisfy their declared coverage policy. An intentionally
empty pre-release catalog is reported as empty, not covered. Corrupt and adversarial fixture
payloads may contain arbitrary bytes while their manifest integrity remains valid.

## Conformance execution

A conformance suite is generic over the subject under test. It validates suite and case identities,
rejects duplicate definitions before execution, sorts cases by identifier, and creates a fresh
subject for every case. Cases execute sequentially through runtime-neutral boxed futures so the
consumer chooses the executor used to poll the suite.

Setup, exercise and teardown are separate failure phases. Teardown is attempted after a case
failure or an unwind panic, and a report retains both the primary and teardown failures. Case pass
state is derived from recorded assertion failures; a caller cannot set an unrelated success flag.
Unwind panics at supported callback and future boundaries become typed failures. Aborting panics,
out-of-memory termination, stack overflow, undefined behavior and process termination cannot be
contained in-process and require later subprocess qualification.

The teardown guarantee applies to a runner future driven to `Ready`, after setup and destruction
of the setup future both complete successfully. Dropping a pending runner is external
cancellation: it synchronously drops the in-flight operation and subject, but cannot await the
factory's asynchronous teardown. Subjects therefore must provide RAII-safe cancellation cleanup.
Production qualification supervisors must drive runs to a terminal report or classify external
cancellation as an infrastructure failure outside that report. A panic while destroying a pending
future is likewise outside in-report unwind containment and requires an outer supervisor boundary.

Reports contain no clock time, duration, thread identifier or backtrace. Their stable ordering and
typed observations make repeated runs directly comparable. Suite status is one of:

- `Empty`: the suite was runnable but declared no cases;
- `Passed`: every declared case passed;
- `Failed`: after a valid nonempty run began, at least one setup, exercise, assertion, teardown or
  caught case-panic failure occurred; or
- `Invalid`: suite-level definition, metadata, duplicate-identity or runner failures prevented a
  valid run from beginning. An invalid report contains no partial case results.

`Empty` is never equivalent to `Passed` and cannot satisfy a release verdict. There is no skipped,
ignored, quarantined or caller-forced success state.

A2 publishes one named empty plugin suite. A3 supplies sixteen real application-protocol cases for
negotiation, command binding, idempotency, event resume/redelivery/acknowledgement, gaps,
backpressure, artifact transfer, prompt freshness, terminal ordering, daemon lifecycle, malformed
input, and bounds. C4 supplies real tool descriptor, schema, exposure, authorization, dispatch,
result, control, deadline, and replay cases. C0 supplies
real journal and replay cases, and C1 supplies real Git/workspace/patch cases covering atomic
candidate creation, stale generation and resource rejection, read-only isolation, rollback history,
and restart reconciliation. C2 supplies fresh-subject process and sandbox cases covering structured
execution, bounded supervision and recovery plus the complete platform-neutral sandbox contract and
backend lifecycle. D1 supplies ten gate cases covering the inspect/edit/run/test path, prerequisite
failure, malformed parser output, revision drift, cancellation, restart recovery, clean snapshots,
retry bounds, artifact evidence, and deterministic aggregation. C7 supplies nine trace/telemetry
cases covering causality, redaction, bounded load, exporter failure, replay, duplicate conflict,
backpressure, shutdown recovery, and non-authority. Negative catalog tests prove implicit gate
success and default-surface secret leakage cannot pass. D2 supplies ten review cases covering
lifecycle, quorum, every independence dimension, duplicate reconciliation, stale revisions,
reviewer-confirmed resolution, external waiver authority, restart, oscillation, and malformed
submissions; its negative oracle proves stale implicit completion fails. Only a nonempty passed
report from a production adapter proves the named contract.

## Ownership boundaries

The A2 provider and tool fixtures verify only generic FIFO capture, exact ordered output,
mismatch, exhaustion, fault scheduling and isolation. The following remain outside A2:

- A3/B3 production messages, codecs, schemas, and compatibility bytes. A2 owns only the
  runtime-neutral A3 protocol subject contract and fixed behavioral cases;
- B0 lifecycle and success semantics;
- B1 authorization, capabilities, leases and budgets;
- B2 acceptance and quality policy;
- C4 production tool calls, results, routing and idempotency storage; A2 now owns their
  runtime-neutral fresh-subject qualification contract;
- C5 provider requests, normalized streams, retry policy and adapters; and
- C0/C1/C2 production persistence, workspace, process, and sandbox implementations;
- D1 gate planning, execution, parsing, persistence, evidence, freshness, and acceptance; and
- D2 review lifecycle, finding conservation, quorum, reconciliation, waiver observation, and replay;
- C7 trace persistence, redaction, projection, buffering, and export. A2 owns only the
  runtime-neutral subject contracts and cases used to qualify those adapters.

Those owners declare their own conformance cases and translate their local types into the generic
A2 mechanics. A verification-class `T` adapter must not become a normal dependency of either A2
crate.

## Required checks

Run focused A2 checks with:

```text
cargo test --package peritus-test-support --all-targets --all-features --locked
cargo clippy --package peritus-test-support --all-targets --all-features --locked -- -D warnings
cargo test --package peritus-conformance --all-targets --all-features --locked
cargo clippy --package peritus-conformance --all-targets --all-features --locked -- -D warnings
```

Because workspace membership, the dependency lock and architecture registry are shared inputs to
formal packages, every A2 integration change also requires the protected proof-impact transition,
the complete `just gate-a` gate, and green Linux, macOS and Windows CI before merge.
