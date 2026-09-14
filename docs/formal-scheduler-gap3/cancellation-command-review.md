# Independent bounded review: cancellation command composition

Reviewer: `/root/gap3_loss_review`, an independent read-only subagent. This report is a technical review artifact, not human approval or protected authorization.

## Verdict

**PASS for the reviewed increment.** I found no remaining correctness, specification, compatibility, or trust-boundary defect in the frozen cancellation-command slice. This verdict is bounded to the source and evidence below; it is not a claim that all GAP03 obligations or the whole scheduler transition system are complete.

I performed a read-only review. I did not edit repository source or run Cargo/Verus; the build artifacts were produced by the root build lane and I inspected their terminal results.

## Review boundary

- Parent checkpoint: `5020fddf692b0894c6c8d188efc3490a3419b8c8`.
- Frozen scheduler snapshot: `target/gap3-evidence/157-cancellation-source.sha256`.
- Snapshot SHA-256: `ada577222b7b8587214ffd3a9580d224964dbf8efc7a56e442180b706b794def`.
- The manifest contains 142 scheduler source/test paths. I reran `sha256sum --check --quiet` against the live checkout and observed exit 0.
- `git diff --check` against the parent checkpoint for the scheduler source also exited 0.

Reviewed implementation paths and their manifest hashes:

- `f9272798c46972420908c619189f6bec0962cc3d7fbc294926e6cca44e37d0bf` — `src/reducer/apply/cancellation.rs`
- `f8a4be69f3e0ed0e1c55585c04ea2c1a1b74536752671abc23cfabbb8d229b4f` — `src/reducer/apply/cancellation/command.rs`
- `4d836fbece5dd42883048a2a94470f00333fad0e1bcf57cfff5abc7101bf156d` — `src/reducer/apply/cancellation/update.rs`
- `7e02d81fa657d39b17aa87d4b4e589cfa209af2988f2d34a8508409a65b68021` — `src/reducer/apply/cancellation/update/frame.rs`
- `7d2265840a1e01e72e4daee94a4ea857df158537c2cdc8eb4e280050f1bd6bde` — `src/reducer/apply/control.rs`
- `2ebfac4b81f0540a0096bf58405a501702383895d44f4e4a743634c22f02c4ca` — `src/state/mutation.rs`
- `59d49e2074628f1e89cd1e72d47877bebdcaf38fe1b4b547f73979274b84e8c1` — `src/state/mutation/reservation_command.rs`
- `4bd464f3ae65ad41926ed33b68b06a860327963a639cbc47948905ab63b52528` — `src/state/mutation/reservation_command/acknowledge_cancellation.rs`
- `490a6a7d90feb7d4448ea5ef3d8323becf65b7ff4c4d536ddcaf349922408e98` — `src/state/mutation/reservation_command/complete.rs`
- `f6043462637530f778e7e9e36eeccbc64fa6684d97ff84cae80b06af00ab6097` — `src/state/mutation/reservation_command/contracts.rs`
- `31dd4dca6674e1a2779ac3d9fa7ac0edd9054483822519a7d47ee0bf93d68747` — `src/state/mutation/reservation_command/outcome.rs`
- `37371ca79ff936a3e295e3098df5d694b2029209c3411ff45c2bc697d922687f` — `src/state/mutation/work_update.rs`
- `3981cb44be02d81794bc462070878cf0d338e393c4bd3fc1dfa6173afe59e07e` — `src/state/mutation/work_update/release.rs`
- `86320a6ee8c10bc5e464ccfc05037f496ded6f01e0f581a5c66c031ceb3eda0b` — `src/state/mutation/work_update/release/terminal_relation.rs`

Paths are relative to `crates/orchestration/peritus-scheduler/`.

## Findings

No open findings remain in this review boundary.

During qualification I independently identified four concrete blockers: a forbidden reveal of a closed lifecycle predicate, a partial move of the returned reservation, missing witnesses at the two phase-rejection exits, and strict Clippy failures in the new control-flow forms. The frozen snapshot resolves all four without weakening a contract, adding a caller precondition, or adding lint/trust suppression.

## Correctness assessment

The cancellation root kernel now gives a complete result relation for the production operation. Missing and already-terminal roots leave the entire state exactly unchanged. On success, the result is exactly the `WorkCancelled` event for the requested root and descendants flag; its affected IDs are the selected pre-state subsequence, and the final work relation is the exact reservation-sensitive cancellation trace with a complete frame for unrelated state.

The cancellation update loop preserves the actual reducer invariants needed by later commands. Each selected target is proved admissible as `Cancelling` when ownership is retained and `Terminal(Cancelled)` otherwise. Readiness is carried through every update. Work length and every work identity remain fixed, while reservations, workers, used dispatches, and all other state fields are framed. Those facts are sufficient to project `spec_collections_ordered` at both loop exits. The production functions add no `requires`; preservation is stated conditionally from the incoming valid-state predicates.

Terminal release now binds the returned reservation to the exact removed index and dispatch/work identities, binds the target work to the exact terminal payload, and frames every other scheduler field. Removing one element from an ordered reservation sequence preserves reservation ordering; lifecycle-only work mutation preserves every work identity; the frame preserves worker and used-dispatch ordering. The release contract therefore exposes both reducer readiness and collection ordering under the existing valid-state and target-existence premise.

The complete and cancellation-acknowledgement kernels expose exact outcomes under `spec_reservation_reducer_ready && spec_collections_ordered`. Wrong command, absent dispatch, missing work, unacknowledged/non-running completion, and non-cancelling acknowledgement are exact no-op outcomes. Successful completion is tied to the supplied result digest and exact terminal release. Successful acknowledgement is tied to `Cancelled` and the exact released ownership. The post-release “work disappeared” acknowledgement branch is proved impossible under the valid-state premise. The existing public lifecycle regression also checks that a completion arriving after a released dispatch is rejected as `UnknownIdentity` and leaves state unchanged.

The runtime changes preserve the checkpoint behavior. The cancellation helper replaces the inline root checks and event construction with the same observable errors/event. Its removed “affected work disappeared” branch is unreachable because affected IDs are selected from retained work and lifecycle updates preserve the identity layout. Completion, acknowledgement, and release retain their lookup, phase-check, removal, and terminalization order; the additional code is proof/spec structure and module splitting.

Manual source inspection found no `external_body`, `assume`, `admit`, axiom, executable proof precondition, or trusted-method shortcut in this slice.

## Evidence inspected

| Artifact | Observed result | SHA-256 |
|---|---|---|
| `157-cancellation-source.sha256` | 142-path manifest; live check exit 0 | `ada577222b7b8587214ffd3a9580d224964dbf8efc7a56e442180b706b794def` |
| `158-cancellation-final-verus.log` | strict verification: 505 verified, 0 errors; 27.13 s | `21b04242aee858b7cb4ffaab0d027f24e465bd894af391d543ae43e0951e7cc2` |
| `159-cancellation-final-tests.log` | scheduler tests: 70 passed, 0 failed, 0 ignored | `a5494e6aad5b0cd1eeeb007ac292a1fe9cce80dcce27f48a22ae48e2debebae4` |
| `156-cancellation-final-clippy.log` | strict Clippy completed successfully | `47d64871359a59f3448af2e65af21f0b651919669dc2186e90adf3eb6eb91ba5` |
| `160-cancellation-final-format.log` | formatting check completed successfully; empty log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

## Limits of this verdict

The exact complete/acknowledgement result relations deliberately apply from an incoming state that is both reservation-reducer-ready and canonically ordered. This review does not claim exact malformed-state classification.

This increment does not by itself prove the entire public reducer loop, event replay, command-history/fence composition, daemon durability, protected-trust inventory, or every remaining GAP03 obligation. It establishes the bounded cancellation root/update/event, exact terminal release, and complete/acknowledgement contracts described above.
