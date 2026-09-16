# Abrupt recovery expansion evidence — 2026-09-11

## Execution contract

- Base pilot commit: `b150e2e7`
- Host: Linux x86_64, kernel `6.17.1-300.fc43.x86_64`
- Toolchain: Rust 1.97.1 with `CARGO_BUILD_JOBS=2`
- Child watchdog: 10 seconds per barrier
- Child control: exact `std::process::Child` handle; kill followed by wait; drop guard repeats cleanup on panic
- Storage: fresh temporary state and repository for every case
- Network and credentials: no external provider or credential use

## Explicit cancel followed by abrupt process death

The owned child starts a product run against a local stalled provider. After the provider records its request, the child calls `cancel`; only after the updated product record is durably written does it publish `cancel-persisted.barrier`. The controller then kills and reaps that exact child process.

The restart oracle decodes the persisted product record, requires `Cancelled`, installs it into a fresh service, invokes interrupted-run recovery, and independently requires zero new provider requests. Fixed identities use bytes `f1` through `f5` for providers, run, and workspace.

Compatibility repair: `user_cancelled` is a default-false persisted JSON field. Old records retain their prior recovery behavior; records written after explicit cancellation restore directly as `Cancelled`. The earlier live flag alone covered graceful shutdown but could not carry intent across process death.

The regression was applied without the repair to revision
`16d5fa8701536180595f80dd631478b4827ee11d`. Three fresh test processes each created a new
repository, state directory, owned child, and cancellation barrier. All three exited 101 with the
same persisted-state signature: `left: RecoveryRequired`, `right: Cancelled`. The first fixing
revision is `6dd8059b9ef189b1d8f954b415d83ee252148e8c`.

```text
timeout 90s env CARGO_BUILD_JOBS=2 cargo test -p peritus-daemon abrupt_cancel_survives_process_termination -- --nocapture
1 passed; 0 failed
```

The separately reviewed forward-migration and safe-downgrade procedure is recorded in
[`cancellation-record-compatibility.md`](cancellation-record-compatibility.md). A direct downgrade
while a `user_cancelled = true` recovery record exists is unsafe.

## Receipt Started and Completed process termination

For each mode, an owned child writes its durable receipt, writes and syncs one line to an independent effect counter, optionally completes the receipt, then publishes an exact barrier and parks. The controller kills and reaps it.

- `Started` command receipt: restart returns durable ambiguity and the effect counter remains one.
- `Completed` workspace receipt: restart replays the result and the effect counter remains one.

```text
timeout 90s env CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner process_termination_preserves_receipt_decisions -- --nocapture
1 passed; 0 failed
```

These tests establish process-crash behavior, not power-loss behavior. They do not weaken the existing complete-corruption negative control or claim that a synthetic counter is an external transaction.

## Confirmed defect: repeated failed startup narration

The fixture restores `RecoveryRequired` work into a service that deliberately lacks the original workspace. Recovery admission therefore fails before provider dispatch. Calling startup recovery twice produced two identical durable restart messages while the provider request count remained one from the original pre-crash attempt.

All three fresh pre-fix runs exited 101 with `left: 2`, `right: 1`. The repair suppresses only a consecutive identical restart notice. Intervening run activity permits a later interruption notice.

```text
timeout 20s env CARGO_BUILD_JOBS=2 cargo test -q -p peritus-daemon repeated_failed_recovery_admission_does_not_duplicate_restart_narration -- --nocapture
```

After the repair, the focused recovery matrix reports five passed, one intentionally ignored subprocess fixture, and no failures.

## Remaining cases

Product-record and resume-state write/fsync/rename failure injection and admitted follow-up versus finalization remain unexecuted. Command stdin/signal/cancel versus terminal exit is covered by existing command-runtime tests but was not expanded with a new process-crash schedule here. No AgentDriver behavior was tested in this lane.
