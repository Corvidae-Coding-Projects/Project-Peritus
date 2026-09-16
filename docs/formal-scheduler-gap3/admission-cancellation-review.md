# Independent bounded review: admission, cancellation, and individual loss release

Reviewer: `/root/gap3_loss_review`, an independent read-only subagent. This is a technical review artifact, not human approval or protected authorization.

## Verdict

**PASS for the reviewed increment.** I found no remaining correctness, specification, compatibility, or trust-boundary defect in the frozen work-admission kernel, cancellation queue/non-resurrection composition, or single worker-loss release. This verdict is bounded to the source and evidence below. It does not establish the complete worker-loss loop, the whole public reducer/replay composition, or completion of GAP03.

I reviewed source and retained evidence without editing production code or running Cargo or Verus. The root build lane produced the gate logs. My only repository write is this review artifact.

## Review boundary

- Comparison checkpoint: `39f56c63663270e5664f938ec1dc95f81b5bacf7`.
- Frozen source manifest: `target/gap3-evidence/182-admission-cancellation-source.sha256`.
- Manifest SHA-256: `dae05abf83eea5d194cb3f6261ac4fcac64e9aa5cf6bb4dde178e49b429f205b`.
- The manifest contains 154 scheduler source and test paths. I ran `sha256sum --check --quiet` against the live checkout and observed exit 0.

The reviewed production paths include `reducer/apply/admission*`, the ordinary admission integration in `reducer/apply.rs`, the queue and non-resurrection additions under `reducer/apply/cancellation*` and `state/mutation/reservation_command/contracts`, the individual loss release under `reducer/apply/loss/release_one*`, and their state/work invariant support. I also reviewed the associated `work_admission`, `queue_recovery`, `cancellation_tree`, and `worker_loss_outcomes` regressions.

## Findings

No open findings remain within this review boundary.

The admission kernel preserves the existing runtime decision order: scheduler draining, version-appropriate queue capacity, duplicate work identity, revision, global resource conflict, dependency, parent, worker eligibility, and ordinal overflow. It constructs the same queued or dependency-waiting record, ordinal, and event as the prior ordinary implementation. Lost and draining workers remain eligible witnesses until removal, matching the existing behavior. The ordinary wrapper maps every kernel rejection to the corresponding existing error and detail.

The success contract fixes the inserted record and event, the retained and post-state lengths, the exact ordinal increment, all unrelated state, reducer readiness, and canonical collection ordering. Its queue proof accounts for both formats. Version 2 admission pressure increases by exactly one and remains within `Q`; the legacy proof partitions waiting and active work, uses ready-state ownership to bound active work by reservations, and derives occupancy within `Q + A`. The public production function introduces no hidden precondition.

Cancellation updates now prove that both strict and legacy queue measures cannot increase. An active selected work item moves to `Cancelling`; an inactive selected item becomes `Terminal(Cancelled)`; unrelated records and their identities are framed. The composed update preserves readiness and ordered collections. The non-resurrection contracts then tie a cancelling dispatch to the actual completion and acknowledgement behavior: completion before acknowledgement is a no-op rejection, acknowledgement releases the exact reservation and terminalizes the exact work as cancelled, and a later completion is rejected without changing state. The public lifecycle regression exercises that sequence.

The individual worker-loss release binds classification of the pre-state record to the exact removed reservation and exact work transition. It retains cancellation dominance, the strict `RetrySafe` attempt boundary, ambiguous/fail terminal mapping, dispatch/work identity, result digest, and the frame for unrelated scheduler state. Its production integration preserves the existing lookup, error, digest, and outcome behavior. This checkpoint intentionally contains no proof of the complete worker-loss batch or loss-specific queue preservation.

The reviewed code uses finite structural scans or existing ordered operations and adds no adverse runtime-complexity regression relative to the prior whole-scan algorithms. Source inspection found no `external_body`, `assume`, `admit`, axiom, executable proof precondition, or trusted-method shortcut in the reviewed slice.

## Evidence inspected

| Artifact | Observed result | SHA-256 |
|---|---|---|
| `182-admission-cancellation-source.sha256` | 154-path manifest; live check exited 0 | `dae05abf83eea5d194cb3f6261ac4fcac64e9aa5cf6bb4dde178e49b429f205b` |
| `183-final-strict-verus.log` | strict verification: 539 verified, 0 errors; 30.45 s | `6c7825b7633f677dce08524cc65c0f4ac6dcc100dbf51e909384d9dc50a8196d` |
| `184-final-scheduler-tests.log` | scheduler tests: 74 passed, 0 failed, 0 ignored | `40bd85c2f283837768cab0d9b4567b710bbb7078914084dc61aad3d6a9410336` |
| `176-scheduler-clippy.log` | strict Clippy completed successfully | `02f95410fa8ee061a5f5750693acc1a9be55d06d566ba71ca20ebf2d56f333e1` |
| `181-final-format-check.log` | formatting check completed successfully; empty log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `185-final-architecture-check.log` | 84 packages and 4532 source files passed | `44ece22148fb456b717bbc5445486978498a04b8755d44b7df99a11898074614` |
| `186-final-ordinary-api-check.log` | 3619 formal-boundary files and 14761 ordinary-safe executable entry points passed | `77d85ada8ca90959418d9580e6fed26cd4b4fee09dbbf341d31eb9f96dee0aa8` |

At the inspected draft-PR head, hosted checks showed no failure conclusion, but the hosted matrix was still queued or running. It is therefore not evidence for a hosted PASS.

## Limits of this verdict

The admission kernel is wired into the ordinary reducer and its mapping is behavior-preserving, but this review does not claim a formal relationship through every outer command/error wrapper, public `decide`, decoded-state caller, or replay path.

The cancellation queue and acknowledgement/completion facts are established from their stated predicates. The formal link from the successful root cancellation command to the non-resurrection chain's initial cancelling-dispatch premise remains open, as do the outer reducer, replay, and caller relationships.

Worker loss remains incomplete beyond the reviewed individual release: loss-specific queue preservation, the complete batch/loop, aggregate resource deltas, worker mutation, and the successor `WorkerLost` event relationship are outside this checkpoint. Remaining transition/replay termination obligations, source-inventory/protected-trust reconciliation, and terminal hosted qualification are also outside this bounded PASS.
