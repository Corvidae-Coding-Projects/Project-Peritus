# H1 resilience qualification

H1 is a release qualification, not a component self-test. Its subject is one exact integrated
Peritus release-candidate build, identified by digest and exercised through real daemon, storage,
workspace, provider, tool, worker, and restart controls. A fake subject may validate the harness;
it cannot produce release evidence.

The `peritus-resilience` crate owns the runtime-neutral qualification model. A release adapter owns
all operating-system and implementation-specific effects. The runner always creates a new isolated
subject for each scenario, executes scenarios in bytewise identifier order, reconciles the result,
consumes the subject through cleanup, and binds the complete report to a canonical SHA-256 digest.

## Production coverage

The immutable catalog has 43 scenarios:

- before-durable-commit and after-durable-before-ack crashes for journal, blob, snapshot, lease,
  patch, gate, and promotion boundaries;
- corruption or hash divergence in the journal, blob, snapshot, projection, acceptance evidence,
  and harness-promotion state;
- disk exhaustion at journal append, blob finalization, and snapshot commit;
- provider, tool, and worker death while work is owned;
- provider, tool, and worker retry-budget exhaustion;
- daemon death during all eleven active E0 phases, from pending writer publication through pending
  kernel acceptance;
- host reboot with an outstanding effect, after durable commit before acknowledgement, and during
  startup reconciliation.

The adapter must prove that each requested failpoint was actually reached. Merely restarting an
idle daemon or returning the expected enum does not satisfy the subject contract.

## Required direct observations

Each case reports the prepared journal head, exact armed fault, documented recovery outcome,
terminal/acceptance truth, journal and referenced-object integrity, projection state, exact
corruption detection, mutation admission, ownership reconciliation, orphan scan, retry counters,
resource counters, temporary-object count, and a canonical six-step lifecycle chronology.

Six content-addressed evidence classes are mandatory per case: fault control, journal integrity,
recovery, ownership/orphan scan, resource accounting, and final authoritative state. The standard
native adapter retains each raw evidence file under a private, subject-addressable artifact root
and verifies its relative path, regular-file type, byte count, and SHA-256 before the deterministic
report retains its typed ID and digest.

## Non-bypassable verdict

`Ready` is derived only when the built-in production profile runs every case and every case passes
execution, private invariants, evidence coverage, and cleanup. A caller cannot construct a ready
report or convert a custom catalog into production evidence through the public API.

The invariant evaluator withholds readiness for false or contradictory acceptance, journal
divergence after a crash, missed corruption, mutation admitted against corrupt authority,
unverified referenced objects, leaked temporary objects, absent ownership scans, non-conserving
ownership counts, unaccounted work, remaining orphans, retry or resource overruns, incomplete
evidence, noncanonical chronology, and incomplete cleanup. Indeterminate work is allowed only when
it is explicitly counted and the run remains non-success; it can never support acceptance.

## Cancellation and cleanup

The runner owns a cancellation token for the current fresh subject. Dropping the runner future
cancels that token before dropping the in-flight operation. A completed run calls asynchronous
factory cleanup once for every created subject and validates its resource and ownership result.
Adapters must also use RAII for process trees, temporary roots, failpoint registrations, quota
devices, and reboot controllers because Rust cannot await cleanup from `Drop`.

No spawned operation may be detached. Cleanup failure, panic, unsupported fault control, or an
unobserved terminal result is infrastructure failure and therefore `NotReadyForProduction`.

## Release adapter responsibilities

`NativeResilienceFactory` supplies the reviewed process boundary. It copies and re-digests the
controller executable inside every fresh subject, launches it with cleared environment state,
owns the complete process tree, applies stage duration and output limits, and cancels synchronously
when the runner future is dropped. One controller remains alive for all four stages so that its
external host or VM control session can survive the fault it invokes. Cleanup is complete only
after the controller exits, all descendants are gone, the private root is removed, and the raw
artifact root remains available for H4.

The controller is invoked as follows; values following each option are supplied by the adapter:

```text
CONTROLLER --serve \
  --candidate-executable FILE \
  --subject-root ROOT \
  --artifact-root ROOT \
  --instance-id ID \
  --subject-id ID \
  --build-sha256 SHA256 \
  --executor-sha256 SHA256
```

It reads one compact JSON request per line from standard input and writes exactly one JSON response
line to standard output. Progress and diagnostics belong on standard error. Requests and responses
follow [`native-controller-request-v1.schema.json`](../resilience/schemas/native-controller-request-v1.schema.json)
and [`native-controller-response-v1.schema.json`](../resilience/schemas/native-controller-response-v1.schema.json).
Each response repeats the stage, sequence, fresh instance, scenario, and exact inner-request digest.
The four stages are `prepare`, `inject`, `recover`, and `cleanup`; the controller exits after its
cleanup response. Unknown fields, stale identities, malformed values, excessive output, timeouts,
unexpected exit, missing evidence, false digests, and surviving descendants fail closed.

## Production controller boundary

The standard production controller qualifies the exact staged `peritusd` executable. It does not
link the daemon's component crates and reproduce their behavior inside the test process. For native
process, dependency, storage, and quota cases, the controller invokes a narrow `peritusd`
qualification-admin surface against a fresh disposable state root. That surface exposes fixed
scenario controls and observations; it does not expose an arbitrary command runner or a way to
construct a successful H1 response.

The persistent controller writes each digest-bound stage request beneath the private subject root,
starts the staged candidate with that request, observes the required checkpoint or terminal state,
and independently retains the candidate output and platform observations. Expected candidate death
is accepted only after the named checkpoint was observed. Starting and stopping an idle process is
not fault evidence.

Host-reboot cases use an external disposable-VM driver owned by the controller. The driver must
bind the guest image, candidate digest, request digest, boot identity, pre-reboot checkpoint, and
post-boot recovery observation. The controller never reboots the developer or CI host, and it does
not substitute a process or container restart for a host reboot. If the reviewed VM driver or its
required image is unavailable, the production campaign is honestly not ready rather than silently
downgrading those cases.

### Current implementation state

The checked-in Rust `peritus-h1-controller` now implements both genuine journal commit routes. For
`h1.crash.journal.before`, the staged daemon builds the production append plan, publishes a
request-bound checkpoint before submission, and is killed. The recovery process runs the production
journal integrity scan and requires zero committed events, aggregate heads, pending outbox claims,
or external effects. For `h1.crash.journal.after-before-ack`, the controller waits for the real
durable effect-before-ack checkpoint, kills the process, and then requires the same staged bytes to
reconcile that exact effect before acknowledging the new live fence. The controller independently
checks the SQLite journal and external effect state in both cases.

Both blob commit routes are also real. Before publication, the staged daemon holds a fully written
production `ArtifactWriteHandle` across its checkpoint; killing it leaves temporary bytes for the
store's restart recovery to remove. After publication, the daemon has finalized and verified the
content-addressed object, committed its metadata, and added a durable evidence-owned reference.
Recovery re-hashes the object, reads its exact bytes, verifies the reference roots, checks that no
temporary file remains, and confirms that the authoritative journal is still valid and unchanged.

Fresh focused diagnostics retained passing one-case reports and six raw evidence files under
`/home/doll/.local/state/peritus/qualification/h1/journal-before.YP5d5R` and
`/home/doll/.local/state/peritus/qualification/h1/journal-after-ack.sCtlT9`. Both reports deliberately
use the `custom` profile and `not-ready-custom-catalog` verdict. The blob reports are retained under
`/home/doll/.local/state/peritus/qualification/h1/blob-before.b7G9TE` and
`/home/doll/.local/state/peritus/qualification/h1/blob-after-ack.UZrw1b`. The other 39 catalog routes fail
closed until their real component failpoints, controlled quota/storage effects, process controls,
or disposable-VM reboot driver are connected. Therefore the full H1 production qualification is
still pending.

The release integration owner must:

1. build and independently review the platform controller executable used by the standard native
   adapter;
2. provision its disposable project, state, quota, and external host or VM resources per case;
3. connect catalog faults to deterministic C0/C1/D1/F0 failpoints and supervised G0 lifecycle
   controls without bypassing the production interfaces being qualified;
4. use controlled quota or faulting storage for disk cases and a supervised host/VM boundary for
   real power-loss/reboot evidence;
5. retain the six evidence records per case and export their digests into `RecoveryObservation`;
6. execute the suite against the locked release build on every release platform where the fault
   mechanism has equivalent semantics;
7. archive the report, canonical digest, build digest, platform identity, and raw evidence bundle
   under the H4 release record.

The repository tests prove the persistent protocol, all 43 fresh-subject translations, retained
artifact verification, cancellation, descendant ownership, and cleanup. They do not prove that an
external platform controller performed a real host reboot or storage fault. H1 is complete only
after that reviewed controller runs the full catalog against the exact release candidate and
produces a `Ready` report with retained evidence.
