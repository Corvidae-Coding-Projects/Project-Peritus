# Independent bounded review: cancellation non-resurrection and replay reconstruction

Reviewer: `/root/gap3_versioned_wire`, independently reviewing source authored outside this agent's worker-loss slice. This is a technical review artifact, not human approval or protected authorization.

## Verdict

**PASS for the reviewed boundary.** I found no remaining correctness, specification, compatibility, or proof-trust defect in the cancellation non-resurrection chain, scheduler phase control, causative-command reconstruction, exact event and transition accessors, cursor mutations, or production replay integration described below.

The initial test review found that two replay-tamper regressions accepted any replay error. The frozen test now requires `SchedulerErrorKind::ReplayMismatch` and the exact `scheduler event differs from deterministic reduction` diagnostic in both cases. I re-reviewed that test-only correction and consider the finding resolved.

I reviewed source and retained evidence without editing production source or running Cargo, Clippy, or Verus. The root build lane produced the cited results. My repository write for this review is this document. This verdict makes no claim about the worker-loss code authored by this reviewer, the complete scheduler beyond the paths below, hosted qualification, human review, or protected-base authorization.

The frozen source boundary is `target/gap3-evidence/211-composition-source.sha256`, SHA-256 `c7a5b16af1112d16e0cb859cd54ef325b85369976acde7b71944ae86cbd2ffc4`. It contains 168 scheduler source and test paths. I ran `sha256sum --check --quiet` against the live checkout after the test formatting correction and observed exit 0.

## Reviewed source identities

Paths are relative to `crates/orchestration/peritus-scheduler/`.

| SHA-256 | Path and reviewed responsibility |
|---|---|
| `050b9bc78b125c417104762de86f7910c3d88d5038f4e7c8cdc04147e00b5a7c` | `src/reducer/apply/cancellation/non_resurrection.rs` — root cancellation to late completion, acknowledgement, and later completion composition |
| `ca864ca5fcdc73853e84a40cb5a57836df4f5d394cd97ee9223e245f12636637` | `src/reducer/apply/cancellation/command.rs` — exact cancellation admission, state effect, and event relation |
| `7641a2471879f25167fdbd6339bc4dc1808dbf43d73407f31ca0bfcf55cdeff2` | `src/state/mutation/reservation_command/contracts.rs` — exact completion and acknowledgement outcome relations |
| `e2bcf0420cd5fbe572a2af443cc821825e6ac5c5545260d241bbb0fb9eaa55e3` | `src/state/mutation/reservation_command/contracts/non_resurrection.rs` — cancellation acknowledgement and completion composition |
| `7734c0a694467b51ed2c01ff130984da11b669775c1dbe8e62d2efb9e8e690e3` | `src/state/mutation/reservation_command/contracts/non_resurrection/classification.rs` — exact race classifiers and terminal-state projection |
| `490a6a7d90feb7d4448ea5ef3d8323becf65b7ff4c4d536ddcaf349922408e98` | `src/state/mutation/reservation_command/complete.rs` — production completion kernel |
| `4bd464f3ae65ad41926ed33b68b06a860327963a639cbc47948905ab63b52528` | `src/state/mutation/reservation_command/acknowledge_cancellation.rs` — production acknowledgement kernel |
| `2c0cc0a0c66843f16b8c3d5799a86d4a127013f8b664ba02cee8c1567b6b9aae` | `src/state/mutation/scheduler_phase.rs` — exact phase classifier, successor, frame, and production mutation |
| `abd75ef418ace0a5e987ce79ed5d494cfba3c29dbedc4b0d8e0712d93bd7bee0` | `src/reducer/apply/control/phase.rs` — exact phase event and typed rejection adapter |
| `3bd039ae02fc91c67a777156dcdc83d8ba75a37c7376eb58e4a0d33dda66da0a` | `src/state/mutation.rs` — review limited to the exact `set_phase` mutation frame |
| `103adb661e4e80410e298ae5d5230c5c8f9090c13d933488bed9471566379ff6` | `src/reducer/reconstruction.rs` — executable event-to-command reconstruction |
| `8ce8ea98f3c59c31e4a869f83c4c2c5df2d2a62df562e9ae1af2110decc895bc` | `src/reducer/reconstruction/relation.rs` — independent exact reconstruction relation |
| `8a52c1dbf3c8869d2baafd19482b3e44bc6f6b594ac18bd895679c8e85f4da97` | `src/event.rs` — exact event/transition accessors and verified clones |
| `1077a5865307c6b678c8cecc86c426c598619a2fe39c508692550deba86a4b6f` | `src/state/mutation/cursor.rs` — exact cursor and digest mutations |
| `85d8fe4110c4f1f9af5fe771284e59215ba603b6c2db73777695db16128b9da6` | `src/reducer.rs` — production start, decide, and replay integration |
| `065aefa285a87b69ba9db077a38166d8f5e30ed0d417d294c2a8f00c63ea71cd` | `src/reducer/apply.rs` — production cancellation/completion dispatch connection |
| `ef2d0695937c8c4c941f5efa62590caeee35ba893721c5b3f859bba5c9d19951` | `src/reducer/apply/control.rs` — production cancellation and acknowledgement dispatch connection |
| `886df300a3aa845c78a45c5eafc29364b67f3cad80334b76eaac7c4f2a612ad0` | `src/worker.rs` — exact reservation dispatch-token accessor used by reconstruction; review was limited to that accessor and its relation |
| `5e9090d52a4927e3e541be1c8ea78f53d696eb0410f3bee1c7fec7aa394bb49d` | `tests/replay_reconstruction.rs` — exact successor-digest and derived-cancellation-payload tamper rejection |

## Correctness assessment

The cancellation command relation fixes both rejection no-ops and the successful event. For a ready, ordered state with a live root dispatch, the successful cancellation retains the exact reservation and moves the root to `Cancelling`; it also fixes the canonical affected sequence and frames unrelated scheduler state. The non-resurrection composition consumes the exact contracts of the real completion and acknowledgement kernels. It proves that completion while the reservation's work is cancelling returns `NotAcknowledgedRunning` without changing state, acknowledgement returns `Applied`, removes the exact dispatch ownership and terminalizes the exact work as `Cancelled`, and a later completion returns `DispatchNotActive` without changing the acknowledged state. The ordinary reducer calls those verified kernels. The outer `SchedulerError` adapters remain ordinary Rust and are outside the formal classifier relation.

Scheduler phase control classifies pause, resume, and drain against every current phase and fixes the exact successor: active/draining pause, paused/draining-paused resume, and active/paused drain. The underlying `set_phase` contract changes only the phase and frames every other scheduler field. Every rejected phase command therefore leaves the complete state unchanged. The event adapter emits exactly the corresponding paused, resumed, or drain-requested event on success and maps each rejection class to the existing stable diagnostic in the ordinary control wrapper.

Reconstruction has an independent declarative `payload_matches` relation. It copies every causative input and deliberately omits derived outputs that reduction must recompute: worker-loss outcomes, cancellation affected identities, and the final scheduler summary. Reservation reconstruction copies both the dispatch identity and idempotent dispatch token through an accessor whose executable return is exactly its specification. The complete command relation also fixes semantics, command and event identities, run and revision, prior digest, and the current replay cursor inputs.

Production replay reconstructs a command, invokes the same `start` or `decide` path used for new commands, then compares the entire reconstructed event with the stored event. That final equality covers derived payloads, all fences, and the successor digest. Mixed semantics and duplicate event or command identities are rejected separately. The reviewed transition accessors expose the exact stored event and state; verified clone relations cover every authoritative event and state field. Cursor advancement changes exactly sequence, last event, and appended command identity, while digest installation changes only the state digest. Both mutations preserve the stated readiness, collection-order, and queue invariants.

The corrected replay regressions first prove that the edited bytes still decode as a valid event, then require the exact replay mismatch category and deterministic-event mismatch detail. This rules out an unrelated earlier error as the reason the tests pass. The tests cover a successor digest and a derived cancellation affected identity; they are representative rather than an exhaustive mutation of every omitted derived event field.

Manual source inspection found no `external_body`, `assume`, `admit`, axiom, solver-limit increase, executable proof precondition, or new lint suppression in the reviewed proof paths.

## Evidence inspected

| Artifact | Observed result | SHA-256 |
|---|---|---|
| `target/gap3-evidence/211-composition-source.sha256` | 168-path frozen scheduler manifest; live check exited 0 | `c7a5b16af1112d16e0cb859cd54ef325b85369976acde7b71944ae86cbd2ffc4` |
| `target/gap3-evidence/204-combined-scheduler-tests.log` | Scheduler runtime tests: 77 passed, 0 failed, 0 ignored | `9c9161e78abb78d1f6f25174968e16f5d8d187ef74b152c407a395cb9169954b` |
| `target/gap3-evidence/205-combined-composition-verus.log` | Strict verification: 590 verified, 0 errors | `3566a8003a4152c3aa18c81cb82b03aeb7fec63218e2e7b39f35ad91c839518c` |
| `target/gap3-evidence/206-reconstruction-mutation.diff` | Negative probe inverted the cancellation descendants reconstruction branch | `723c2a8ca54e7a53082ef4f56a5b8fcef8e9433334bb10a6dfc43dc0cdf64e36` |
| `target/gap3-evidence/206-reconstruction-mutation.log` | Negative probe failed the `payload_matches` postcondition: 589 verified, 1 error, exit 101 | `20583c73677b80383988d82b0223c7886c97299520166a86ca92c686785cc210` |
| `target/gap3-evidence/206-reconstruction-mutation-restoration.log` | Probe source restoration reported byte-identical | `b5d8ff0502eaee159f540265f86b97b00468689195bc1b1eb6af6509a86ffdd6` |
| `target/gap3-evidence/208b-final-composition-format.log` | Formatting check passed; empty log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `target/gap3-evidence/209-final-composition-architecture.log` | Architecture check passed: 84 packages, 4,546 source files | `b87b2ffdb55f851183229431c3435e3d700722b4bf89bdf1104ae82eb2640af9` |
| `target/gap3-evidence/210-final-composition-api.log` | Ordinary API check passed: 3,633 formal-boundary files, 14,763 ordinary-safe entry points | `9eb31e268264d103a8b6e1fffbe067d53f8f8264ba64f52c1e8a1184d6078ca1` |
| `target/gap3-evidence/212-final-composition-tests.log` | Final scheduler tests: 77 passed, 0 failed, 0 ignored, including both exact replay mismatch regressions | `8782821fdd115bf05036777cada9916a199923b782554446302ebe044cd317e5` |
| `target/gap3-evidence/213-final-composition-clippy.log` | Strict Clippy completed successfully | `9655e3fe9d51b40d6afa7baf5acae5eb9d7e09a48fd493845ec53281bc40feaf` |
| `target/gap3-evidence/214-final-composition-verus.log` | Strict verification with `--no-cheating --rlimit 20`: 590 verified, 0 errors; 36.52 s | `b0ddf50e4a8cac2020256b5817c6888907b26f175acf82ff22ba4305966f075e` |

The root lane rechecked `211-composition-source.sha256` after gates 212–214 and observed exit 0; I independently repeated that manifest check and also observed exit 0.

## Limits of this verdict

The root-to-chain theorem is proof-only and is not itself consumed by a larger temporal invariant. Its premises are nevertheless supplied by contracts on the production-called cancellation, completion, and acknowledgement kernels. This review does not formalize the outer reducer's diagnostic mapping or a durable multi-command transaction.

The replay proof fixes command reconstruction and the exact accessors it uses. The production replay loop and cryptographic digest computation remain ordinary executable code whose behavior is covered by runtime tests and full-event comparison. This review does not claim a formal theorem for the complete loop, cryptographic correctness, persistence, daemon recovery, or hosted qualification.
