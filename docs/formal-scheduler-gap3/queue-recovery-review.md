# GAP-03 queue recovery review

The proposed admission pressure is transition-complete for the strict invariant if defined as current `Queued | WaitingDependencies | RetryPending` plus `Reserved | Running` whose `attempts_started < maximum_attempts`, regardless of `RecoveryPolicy`.

Dispatch preserves pressure when another attempt remains and lowers it on the final attempt; acknowledgement, retry-pending-to-queued, and dependency readiness preserve it; explicit retryable failure preserves it for any recovery policy; retry-safe worker loss preserves it; completion, non-retryable failure, non-retry-safe loss, exhaustion, abandonment, and cancellation only lower it. `Cancelling` needs no reserve because `FailWork` rejects it and worker loss terminalizes it. This is the sound strict-`queued_work` repair for new aggregates.

Tightening `AdmitWork` materially breaks replay of previously accepted benign histories, not only the reproduced overfill history. After recoverable A dispatches, old code admits B into the freed visible slot; even if A later succeeds, new replay rejects B at its `WorkAdmitted` event. Decoded checkpoints with that pressure also become invalid if validation adopts the strengthened invariant. Because replay reruns `decide` and schema v1 has no reducer-semantics marker, there is no unversioned way to preserve both strict `waiting <= queued_work` and every old event stream. Rejecting recovery is unsound; changing retryable or worker-loss outcomes breaks command/event correspondence.

If D3 schema v1 is still pre-release with no durable user histories, land the proposed pressure admission now, strengthen decoded-state validation to the same pressure predicate, and retain worker-loss, explicit-retry, and negative decoded-state regressions. If existing journals are a compatibility commitment, use a new binding/schema semantics version: legacy replay retains old admission and validates its actual bound (`waiting <= min(retained_work, queued_work + active_reservations)`), while the new version uses reserved pressure `<= queued_work`. Merely relaxing current validation to `queued_work + active_reservations` is the shortest backward-compatible patch, but it changes the documented “maximum simultaneously queued work” guarantee and should not be represented as the strict fix.

The pressure scan should include `Reserved | Running` even if reservation relations are malformed during inert validation. Later validation checks reservation consistency, while the pressure scan remains total and rejects excess independently.

## Evidence and source references

- Reproduction: `target/gap3-evidence/35-queue-recovery-reproduction.log`; both `tests/queue_recovery.rs` cases fail decoded-state validation after publicly accepted history and successful replay.
- Admission and worker loss: `crates/orchestration/peritus-scheduler/src/reducer/apply.rs:59-175`.
- Exact visible queue count: `crates/orchestration/peritus-scheduler/src/reducer/apply/admission.rs:10-50`.
- Retryable failure transition: `crates/orchestration/peritus-scheduler/src/state/mutation/reservation_command.rs:178-248`.
- Explicit retry transition: `crates/orchestration/peritus-scheduler/src/state/mutation/work_command.rs:41-104`.
- Dispatch attempt increment and final-attempt checks: `crates/orchestration/peritus-scheduler/src/reducer/apply/dispatch.rs:45-134` and `crates/orchestration/peritus-scheduler/src/work/record.rs:202-241`.
- Decoded-state queue bound: `crates/orchestration/peritus-scheduler/src/state/validation.rs:9-35`.
- Deterministic event replay through `decide`: `crates/orchestration/peritus-scheduler/src/reducer.rs:107-135`.
- Durable schema and replay commitments: `docs/d3-scheduler-collaboration.md:101-145` and `crates/orchestration/peritus-scheduler/README.md:8-21`.
