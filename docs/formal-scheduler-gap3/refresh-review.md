# Independent bounded review: scheduler refresh kernels

Reviewer: `/root/gap3_versioned_wire`, independently reviewing refresh source authored by other agents. This is a technical review artifact, not human approval or protected authorization.

## Verdict

**PASS for the reviewed bounded kernels, with the outer dependency loop explicitly remaining open.** I found no correctness, specification, preservation, or termination defect in the exact dependency action selector, the immutable-round collector, the worker refresh step and finite batch, or the iterative worker reservation counter. The production reducer calls the reviewed refresh path after each accepted command, and the runtime regressions exercise the multi-round failure and worker-ownership behaviors described below.

The remaining GAP-03 requirement is an exact contract and termination proof for `propagate_dependencies` and its application of each collected round. The current selector and collector prove what a round contains; they do not prove that the ordinary outer loop applies every change, strictly progresses, stops at dependency closure, or frames the complete state across all rounds. This is tracked work, not a defect claimed against the reviewed bounded kernels.

I reviewed source and retained evidence without editing the reviewed production source or running Cargo, Clippy, or Verus. My repository write for this review is this document. The finalization admission, terminal construction, and terminal mutation files authored by this reviewer are excluded. This verdict also makes no claim about the complete reducer theorem, cryptographic hashing, durability, hosted qualification, human review, or protected-base authorization.

Gate 228c binds this review to 176 frozen scheduler paths. I checked the manifest against the
worktree after the final proof-only dependency-action module split and lint fixes; all entries
matched.

## Reviewed source identities

Paths are relative to `crates/orchestration/peritus-scheduler/`.

| SHA-256 | Path and reviewed responsibility |
|---|---|
| `ce54860f8a1d85ff21017f79059740055184d79ca0e88b11c32e632aafab361c` | `src/state/mutation/dependency_action.rs` — exact dependency state observation, canonical first-failure selection, readiness, and no-change classification |
| `c163f9ebc38ab0986e2e50eb223435e8fd1d5ec7776916f96f5256ab3ca2346f` | `src/state/mutation/dependency_action/proofs.rs` — proof projections used by action selection and actionable/no-change exclusion |
| `e828f10eca507e5d380e1b2b83b09984976ba0c96070c44ac167b518244b12a7` | `src/state/mutation/dependency_round.rs` — complete stable-order filter/map of actionable records from one immutable snapshot, including empty input |
| `2d7473bbf7ca508dbe810b220de174148771e34533034cbaa07f1d52d6ce9283` | `src/state/mutation/worker_refresh.rs` — retained worker identity snapshot, finite batch traversal, complete phase projection, and frame composition |
| `7c35e60360151f9813d3d60e3d0cbdf732f424746a565b2ec015a73ac709d258` | `src/state/mutation/worker_refresh/step.rs` — exact one-worker lookup, ownership count, target phase, mutation, and no-op branches |
| `8f5b25ed2035412f27784d6b94e0087517e024962b6854fbee3fd47300143187` | `src/selection/capacity.rs` — review limited to the iterative `worker_count_from` and whole-sequence `worker_reservation_count` wrapper |
| `aafae42da01347f298940c3ccc420c6a6e1561f52966468770b7842414aa3d88` | `src/state/mutation/worker_update.rs` — review limited to the exact worker phase update and complete-state frame consumed by worker refresh |
| `2b90e10c1b604441bdf726730b43b70991e5c958cbb4042a09066fce75003455` | `src/state/mutation/work_update/wrappers.rs` — review limited to exact phase and terminal updates consumed by dependency propagation |
| `05fc336949d9d3c21068829f4f81ef73c7721e1860dcc9325b95c70861040418` | `src/state/mutation.rs` — module connection and production `refresh`/`propagate_dependencies` call order; outer-loop proof remains open |
| `85d8fe4110c4f1f9af5fe771284e59215ba603b6c2db73777695db16128b9da6` | `src/reducer.rs` — review limited to the production call to `mutation::refresh` after accepted command application and before cursor/digest construction |
| `c592b54e35d66019e8c330e14ab70672f6835d786a45ec1af51e370b02c7ee4e` | `tests/refresh_composition.rs` — reverse-order dependency cascade, canonical first failure, unrelated-work preservation, cancelling ownership, inactive worker phases, and replay |

## Correctness assessment

The dependency selector observes dependencies in the immutable `WorkSpec` sequence and returns immediately on the first retained terminal non-success. Its declarative `first_failed` relation fixes both the selected identity and the absence of a failed dependency at every earlier specification index. A missing or nonterminal dependency prevents `Ready` without being mislabeled as failed. Waiting work becomes ready only when every dependency has exact succeeded evidence; queued work can still detect failure but is never redundantly classified ready. All other work phases are exact no-change cases. Ordered-state lookup uniqueness connects the production binary search result to the quantified dependency predicates.

The final file-budget split moved only Verus proof helpers into
`dependency_action/proofs.rs`, behind `cfg(verus_only)`. The executable selector, its result
shape, and its call from the round collector did not change. The final
`dependency_round.rs` change only reordered imports. Gate 228c then widened one proof-helper
visibility for its existing re-export and made the pure finish helper `const`, with an underscore
parameter rename for the lint. I found no semantic drift in these final diffs.

The round collector scans the retained work sequence once and produces the stable-order filter/map of precisely actionable records from that single state snapshot. The recursive independent relation distinguishes no-change records from actionable records and binds each change to the record identity and exact action. Empty work produces an empty change sequence, and the output length is bounded by retained work length. Because the snapshot remains immutable during collection, the relation does not accidentally mix observations from different propagation stages.

The worker refresh batch copies retained worker identities, traverses that finite vector with `decreases len - index`, and refreshes every worker against the unchanged reservation sequence. Available and busy workers become busy exactly when retained ownership count reaches or exceeds immutable concurrency; draining, lost, and removed workers remain unchanged. All live reservations are counted, including ownership retained by cancelling work. The step relation fixes the selected index and leaves every other worker record equal. Descriptor layout, work, reservations, dispatch history, cursor fields, terminal evidence, readiness, collection order, and queue bounds are preserved. The processed/pending partition proves the final phase equation for every retained worker; it is vacuously complete for an empty worker sequence.

The worker reservation counter starts its cursor at the slice length and iterates backward to the requested starting index. Its invariant equates the accumulator with the mathematical count over `[cursor, len)`. Each decrement exposes the current record as the suffix head and the already-counted region as its tail, then increments only when `WorkerId::same` reports exact equality. Empty and range-end inputs return zero, the accumulator bound prevents overflow, and `cursor - index` proves unconditional termination. The production wrapper uses index zero, so it covers the entire reservation sequence without recursion or input-order assumptions.

Production `decide` clones the accepted prior state, applies the command, then calls `mutation::refresh` before encoded-size admission, cursor advancement, and state digest construction. `refresh` invokes dependency propagation before worker refresh, which is the required order because dependency changes affect work only while worker availability derives from reservation ownership. The work and worker mutation kernels used by these passes retain immutable definitions/descriptors and frame unrelated scheduler state. Ready changes preserve queue occupancy category, while dependency-failure terminalization removes waiting occupancy; the exact whole-loop queue and closure composition remains part of the open outer-loop requirement.

The focused regression constructs canonical work identity order opposite the dependency cascade, forcing more than one propagation round. It checks the first failed dependency in specification order, the downstream failure identity, unrelated work equality, and replay. The worker scenario checks available-to-busy transitions at exact concurrency, counts a cancelling reservation until acknowledgement releases it, restores availability after release, preserves draining/lost/removed workers, and replays the resulting history exactly. These tests substantiate production behavior but do not replace the open outer-loop proof.

Manual source inspection found no trust construct or recursion in the reviewed worker count and worker refresh execution. Final gate 230c verified the frozen selector, collector, worker refresh, counter, and terminal composition with `--no-cheating` and `--rlimit 20`.

## Evidence inspected

| Artifact | Observed result | SHA-256 |
|---|---|---|
| `target/gap3-evidence/228c-refresh-source.sha256` | Final source freeze: 176 scheduler paths; independent `sha256sum --check --quiet` exited 0 | `f3682e9867d5e449f36c6db291a58244716d9df99af38c926adb3531af596aab` |
| `target/gap3-evidence/226c-refresh-format.log` | Final workspace formatting passed; successful command produced no output | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `target/gap3-evidence/227-refresh-architecture.log` | Architecture check passed: 84 packages, 4,554 source files | `4da208cde418c8be5ee8d7c319019a6aeb98194787328695057cc2fd4a411443` |
| `target/gap3-evidence/225-worker-count-mutation.diff` | Negative probe inverted the worker-identity comparison in the reverse counter | `9e9492c6e9154cd9123630293ccfbc7910b65826a4f820a37cf40e0bd5c3ed5b` |
| `target/gap3-evidence/225-worker-count-mutation.log` | Negative probe failed the exact suffix-count loop invariant: 627 verified, 1 error, exit 101 | `a3e46077864cb589149a908becb61fe01bbc90cfbdcfb627d73e1059a97f40ca` |
| `target/gap3-evidence/225-worker-count-mutation-restoration.log` | Probe source restoration reported byte-identical | `b5d8ff0502eaee159f540265f86b97b00468689195bc1b1eb6af6509a86ffdd6` |
| `target/gap3-evidence/229b-final-refresh-tests.log` | Final scheduler runtime suite: 79 passed, 0 failed, 0 ignored | `abe955c72ebd16b412b63aae04bc3239af77afdd2021344c292eb1e6b35e7d6c` |
| `target/gap3-evidence/230c-final-refresh-verus.log` | Final strict verification with `--no-cheating` and `--rlimit 20`: 628 verified, 0 errors; 38.92 s | `f2b14f5ce2893c4257c4d851eef8093f074a9067a1098efd66d1222b4efe3c00` |
| `target/gap3-evidence/231b-final-refresh-clippy.log` | Final strict Clippy passed; 6.67 s | `7d9a02a01d1c7b952d1d8790b03389c8c2c3f47b2760a1fff45cec17076337eb` |
| `target/gap3-evidence/232-refresh-api.log` | Ordinary API check passed: 3,641 formal-boundary files, 14,764 ordinary-safe executable entry points | `69c0d875242ec43df3a6c51d0faacba4b8d21786e83c852f5b11403cebf9be19` |
| `target/gap3-evidence/220-refresh-finalization-tests.log` | Scheduler runtime tests before the last proof-only refactors: 79 passed, 0 failed, 0 ignored | `9b0734f7eea63878fb14ae20771ac141e68fec6eeabbfe97e0756fb555527b0c` |
| `target/gap3-evidence/223-refresh-finalization-clippy.log` | Strict Clippy passed | `89cf376f008d2eeeee73709a075fe121951f5253f31aafcc7c4a0af2070297cc` |
| `target/gap3-evidence/224-refresh-finalization-verus.log` | Strict verification: 628 verified, 0 errors; 38.60 s | `593da35831b329537b9cf427756ef3713c4102cd3d60def0f346329b449ed171` |

## Limits of this verdict

The dependency action and round relations are production-grounded one-pass results under the established ordered-state condition. This review does not claim that they establish a fixed point by themselves. The ordinary `propagate_dependencies` loop currently has no exact composite postcondition or verified progress measure, and its ignored phase-update results are not yet connected to retained-identity uniqueness in a whole-loop proof. That bounded composition is the remaining requirement.

Worker refresh has an exact complete-state relation under the established ready and ordered state condition, plus unconditional finite execution. The worker counter proves exact mathematical count when the reservation invariant supplies its stated premise. This review does not extend either contract to arbitrary forged private state, and it does not identify such state as production-reachable.
