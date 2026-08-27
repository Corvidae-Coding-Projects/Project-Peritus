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
recovery, ownership/orphan scan, resource accounting, and final authoritative state. Raw evidence
remains in the integrated system; the report retains typed IDs and digests.

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

The release integration owner must:

1. add `crates/app/testing/peritus-resilience` to the root workspace and architecture registry;
2. implement a black-box adapter that provisions a disposable project and state root per case;
3. connect catalog faults to deterministic C0/C1/D1/F0 failpoints and supervised G0 lifecycle
   controls without bypassing the production interfaces being qualified;
4. use controlled quota or faulting storage for disk cases and a supervised host/VM boundary for
   real power-loss/reboot evidence;
5. retain the six evidence records per case and export their digests into `RecoveryObservation`;
6. execute the suite against the locked release build on every release platform where the fault
   mechanism has equivalent semantics;
7. archive the report, canonical digest, build digest, platform identity, and raw evidence bundle
   under the H4 release record.

The crate intentionally does not claim that source compilation or these tests have passed. H1 is
complete only after the integrated adapter is wired and the full release-candidate run produces a
`Ready` report with retained evidence.
