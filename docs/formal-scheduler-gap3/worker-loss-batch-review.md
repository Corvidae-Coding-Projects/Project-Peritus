# Independent worker-loss batch review

Reviewer: `/root/gap3_loss_review`, an independent read-only Sol 5.6 xhigh subagent. This is a technical review of the bounded source increment, not human approval, protected-branch authorization, or a whole-GAP03 completion verdict.

## Verdict

**PASS for the reviewed worker-loss batch increment.** I found no correctness, specification, runtime-compatibility, resource, queue-bound, or termination defect in the frozen source described below.

The production `LoseWorker` path still takes the exact ordered projection of reservations owned by the requested worker and computes `sha256(dispatch_id.as_bytes())` for each dispatch. It passes that concrete plan to the verified batch kernel. The prior error kinds and detail strings, outcome order, per-policy classification, terminal payloads, and final `WorkerLost` event remain unchanged.

The kernel theorem is accurately conditional: it applies when reducer readiness and collection ordering hold, the worker is a retained non-Lost/non-Removed target, and the supplied plan's dispatch projection equals the exact selected pre-state projection. The ordinary production wrapper supplies that plan; the kernel does not claim that every arbitrary supplied plan equals the projection. Under those conditions, errors are excluded and the result is an exact finite release trace followed by an exact descriptor-preserving worker phase update to `Lost`.

The release-trace relation is nonvacuous. It fixes both endpoints, requires one more state than plan entries, requires exactly one outcome per plan entry, and constrains every adjacent state pair with the exact individual release relation. For an empty plan, those constraints force the released state to equal the initial state. Selection proofs establish origin, order, uniqueness, and initial existence; the loop proves that each later distinct dispatch survives earlier removals.

Each release preserves reservation reducer readiness and canonical collection ordering. The queue proof shows that neither the strict V2 `waiting || can_return` pressure nor the legacy `waiting || active` occupancy can increase, and the batch carries the selected queue bound through every step and the final worker-only update. Exact reservation removal and exact work lifecycle mutation preserve the ownership/resource invariant; the worker update retains every non-worker field and the complete descriptor. All executable loops and recursive trace/selection/count proofs have explicit finite decreases.

The public regression covers all five loss outcomes, cancellation dominance, the strict retry-attempt boundary, Reserved and Running work, canonical output order, an unrelated worker and reservation, exact remaining resources, immutable work fields, exact failure/cause digests, replay, checkpoint round-trip, empty loss, and the exact missing/Lost/Removed rejection behavior. Its matrix exercises three recovery policies, two attempt limits, and both started states.

## Frozen source binding

This review is bound to [`211-composition-source.sha256`](evidence/211-composition-source.sha256), which contains 168 scheduler source/test paths and has SHA-256:

```text
c7a5b16af1112d16e0cb859cd54ef325b85369976acde7b71944ae86cbd2ffc4
```

The worker-loss-specific entries include `reducer/apply/loss.rs`, the complete `loss/batch` and `loss/release_one` trees, `state/mutation/worker_update.rs`, `worker.rs`, the ordinary `reducer/apply.rs` integration, and `tests/worker_loss_outcomes.rs`. The manifest records their exact individual hashes and passed a live `sha256sum --check --quiet` after the final gates.

## Evidence inspected

- [`214-final-composition-verus.log`](evidence/214-final-composition-verus.log), SHA-256 `b0ddf50e4a8cac2020256b5817c6888907b26f175acf82ff22ba4305966f075e`: strict `--no-cheating --rlimit 20` verification completed with **590 verified, 0 errors** in 36.52 seconds.
- [`212-final-composition-tests.log`](evidence/212-final-composition-tests.log), SHA-256 `8782821fdd115bf05036777cada9916a199923b782554446302ebe044cd317e5`: **77 passed, 0 failed, 0 ignored** across the scheduler tests; `worker_loss_outcomes` passed 2/2.
- [`213-final-composition-clippy.log`](evidence/213-final-composition-clippy.log), SHA-256 `9655e3fe9d51b40d6afa7baf5acae5eb9d7e09a48fd493845ec53281bc40feaf`: strict Clippy completed successfully.

I inspected these retained logs and did not run a second build lane.

## Boundaries

The SHA-256 computation and error translation remain in the ordinary wrapper. Their behavior is source-equivalent to the pre-increment implementation and exercised by public tests, while the verified kernel proves exact propagation of the supplied digest values. This review does not claim an end-to-end theorem for that wrapper or for the whole reducer/replay caller chain.

I did not review or authorize the reconstruction, cursor/digest, transition, cancellation, admission, phase-control, dependency-refresh, or other scheduler changes merely because they share the 211 manifest. Those components retain their own reviews and stated open work. This bounded verdict does not discharge protected trust/inventory requirements or complete GAP03.
