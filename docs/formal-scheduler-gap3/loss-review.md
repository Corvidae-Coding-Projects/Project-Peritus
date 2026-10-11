# GAP03 scheduler loss and fence review

## Verdict

**Bounded PASS for the reviewed loss, admission-count, resource-accessor, command-clone, and fence increment at the frozen source snapshot.** I found no source-level correctness or behavior-preservation defect in that slice.

This is not a completion verdict for all of GAP03 or for the package test suite. Two intentionally retained queue-recovery regressions still fail, and the worker-loss proof is helper-level rather than an end-to-end theorem over the complete release loop and successor state.

Review mode was read-only. I did not edit repository files, build, run tests, or consume a separate Verus lane. I inspected the frozen source, the base source at `a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8`, and the root-owned evidence logs. Repository source snapshot integrity was independently rechecked with `sha256sum --check --quiet target/gap3-evidence/60-source-snapshot.sha256`.

## Findings

### No defect found: exact pre-transition worker-loss selection and classification

- `src/reducer/apply/loss.rs:11-17` defines the selected sequence as the stable filter of reservations whose worker identity equals the lost worker, followed by projection to dispatch identity. The implementation at lines 20-60 walks the retained reservation sequence once, compares verified worker identity, and preserves order. Its postcondition is exact sequence equality, not set membership or length equality.
- `src/reducer/apply.rs:68` obtains that sequence before any reservation, work, resource, or worker mutation. Each dispatch is classified from the retained pre-release `WorkRecord` at lines 71-77; the outcome is pushed only after the corresponding release succeeds at lines 78-107.
- `src/reducer/apply/loss.rs:64-100` gives every outcome exact dispatch and work identity. Cancellation has first priority. Otherwise, `RetrySafe` is requeued exactly when `attempts_started < maximum_attempts`; equality with the attempt ceiling is exhausted. `Ambiguous` and `Fail` map exactly to their terminal classifications. Runtime classification at lines 103-121 has the same case split.
- The release mapping in `src/reducer/apply.rs:78-103` remains exact: requeue to `Queued`, cancellation to `Cancelled`, exhaustion and failure use the same dispatch-derived SHA-256 digests, and ambiguity retains the same dispatch identity.
- `tests/worker_loss_outcomes.rs` exercises a deliberately non-ordered creation sequence and expects canonical dispatch order `[50, 60, 70, 80, 90]`, all five outcome variants, both `Reserved` and `Running` records, strict retry-boundary behavior, exact work identities, preservation of the other worker/reservation, resources, used-dispatch history, immutable work spec, attempts, and enqueue ordinals, replay, and checkpoint round-trip. Its cancellation matrix covers every recovery policy, maximum-attempt fixtures 1 and 2, and both started states: 12 cases.

### No defect found: direct `LossOutcome` verification and clone contract

- `src/event.rs:14-52` opts the real Rust enum directly into verification with `#[cfg_attr(verus_keep_ghost, verifier::verify)]`; it does not use `external_body`, an assumed contract, or lint suppression. All fields remain documented.
- The pinned Verus implementation at `/home/doll/.cargo/git/checkouts/verus-e4ebf515fa1de14c/92f466f/source/rust_verify/src/external.rs:227-235` includes `eattrs.verify` in `opts_in_to_verus`; lines 278-284 select `VerifState::Verify`. That pinned file hashes to `a431c6fdc7ba3461e749502bbbd863ddfe3df4f8e8fa05c078b544c6702a4406`.
- `src/event.rs:56-78` manually clones every variant and proves `result == *self`. This preserves ordinary Rust clone behavior while giving Verus the exact equality needed downstream.

### No defect found: admission count and resource projection

- `src/reducer/apply/admission.rs:10-50` counts exactly `Queued | WaitingDependencies | RetryPending`, matching the base iterator predicate. The verified loop proves equality with the filtered retained-work sequence and proves the count cannot exceed retained work length.
- `src/resource.rs:176-185` keeps the same borrowed-slice API and runtime result. It projects the existing `ResourceVector` type invariant to the returned entry sequence, proving exact sequence equality and canonical entry ordering without adding a caller precondition.
- `src/work.rs:105-111` keeps the same public `const fn recovery(&self) -> RecoveryPolicy`; its new postcondition ties the runtime value to the actual stored recovery field and adds no precondition.

### No defect found: exact command fences, priority, and clone coverage

- `src/reducer/fences.rs:23-81` specifies 16-byte and 32-byte equality as direct array equality and checks every index with finite decreases. The runtime predicates therefore cannot accept prefixes or partial identities.
- `src/reducer/fences.rs:83-130` compares all seven `RevisionTuple` components: acceptance specification, harness, workspace, workspace generation, workspace revision, policy, and provider profile.
- `src/reducer/fences.rs:132-165` defines used-command membership by exact 16-byte identity over the entire retained sequence. Lines 167-220 combine exact run, all revision fields, sequence, exact predecessor, exact digest, unused command identity, and non-genesis command kind.
- `src/reducer/fences.rs:223-267` proves and implements the required ordered decision: terminal first, then retained-history length `>= 65_535`, then stale fences, then acceptance.
- `src/reducer.rs:139-156` projects those internal results to the same public error kinds, recovery actions, and diagnostic strings as the base implementation. The public `SchedulerCommand::new` predecessor-shape validation is unchanged.
- `src/command.rs:27-38,150-164` relates and clones all eight command fields. `src/command/kind/clone_impl.rs:11-152` covers every command variant and every payload; the nested binding, worker descriptor, and work-spec clone relations were also inspected and are field-complete. No public precondition was introduced.
- `tests/command_fences.rs` covers acceptance of an exact current command, all ordinary stale dimensions, exact duplicate command identity, a genesis command after start, all seven revision components independently, every byte of the 16-byte run/predecessor/command identities, every byte of the 32-byte digest, unchanged input, and terminal-over-stale priority. History-over-stale is established by the exact proof and its negative mutation; constructing a 65,535-entry public test fixture was intentionally avoided.

### Behavior preservation relative to the base

I compared the frozen runtime paths with `git show a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8:<path>`. The loss refactor extracts the former inline reservation filter and outcome case split without changing ordering, thresholds, terminal variants, digest inputs, error strings, or mutation order. Fetching emitted work identity from the located record is equivalent because the state lookup is by that reservation's work identity and the retained record owns the same identity.

The fence refactor preserves the base decision priority and public diagnostics. Its explicit byte loops replace derived identity equality with equivalent exact array equality; its retained-command scan replaces exact slice membership without changing semantics. The admission counter, recovery accessor, resource entries accessor, and manual clones preserve their prior runtime results and public signatures.

## Evidence

- Final strict Verus: `target/gap3-evidence/54-restored-fences-loss-verus.log:1909-1910` reports **448 verified, 0 errors**, then a finished build.
- Final strict Clippy: `target/gap3-evidence/57-scheduler-clippy.log:16` finishes successfully with no warning/error entries.
- Final exact-source package tests: `target/gap3-evidence/58-scheduler-tests-final.log` records **45 passed, 2 failed, 0 ignored** in total. `command_fences` is 5/5 at lines 28-37 and `worker_loss_outcomes` is 1/1 at lines 152-157. The only failures are the two `queue_recovery` tests at lines 58-82; the command therefore correctly exits failed and names only that target at line 165.
- Format check: `target/gap3-evidence/59-format-check.log` is empty, as expected for a successful quiet `cargo fmt --check`; the root-owned command recorded exit 0. Independent `git diff --check` over the reviewed files also produced no output.
- Frozen-source manifest: `target/gap3-evidence/60-source-snapshot.sha256` covers all 34 modified/new scheduler source and test files and passed a live quiet hash check after logs 54, 57, 58, and 59.

The four guarded proof mutations each changed one boundary, produced exactly **447 verified, 1 proof error**, failed compilation, and were restored before the final proof and source snapshot:

1. `50-descriptor-max-mutation.log:1200-1207,1918-1919`: rejecting the valid maximum concurrency violates the descriptor admission postcondition.
2. `51-fence-history-mutation.log:1181,1924-1925`: allowing history length exactly 65,535 violates the fence admission postcondition.
3. `52-loss-owner-mutation.log:1181,1915-1916`: inverting the lost-worker ownership filter violates the selection loop invariant.
4. `53-loss-attempt-mutation.log:1181,1923-1924`: allowing retry at the exact maximum-attempt boundary violates the classifier postcondition.

The final strict proof (`54`) and source manifest (`60`) are later than all four mutation logs. The saved mutation diffs show the one-line changes, and the frozen hashes below equal the manifest entries.

## Frozen reviewed-source hashes

```text
712e3a0f21bccfab15432f05dfd8ff625dc159cff912144393eb784f44eb43ae  crates/orchestration/peritus-scheduler/src/reducer/apply/loss.rs
23a6792f59ad1f529c57d69d04e1f12484567ad0b703d22394003c7701f1721a  crates/orchestration/peritus-scheduler/src/reducer/apply.rs
66f0152fa9c13d79e78e7cca6728149a6afbe8c1d33a09069d651d75313bc6ce  crates/orchestration/peritus-scheduler/src/event.rs
f45c74743b4a5520d48ef763268b917c7b2d11462d863616b7e6129f6d616f0e  crates/orchestration/peritus-scheduler/src/work.rs
a61b88c7834964238005d4b7197d55ecbec38e6b558e5e72824c64f0de93888c  crates/orchestration/peritus-scheduler/src/reducer/apply/admission.rs
91a99a066ed0cd0a45ae962db33dcbb922842c19b39867c3f4a1828cabb68dd9  crates/orchestration/peritus-scheduler/src/resource.rs
9aad7aef13bc6f49ffd1e2e5f4631b81e67b388a549a1a662980ffef749bfdd4  crates/orchestration/peritus-scheduler/src/command.rs
c64fef3835aeb65fb61d697efbe3afb4384a8a03c3c8ad69e1ce596d123a09ef  crates/orchestration/peritus-scheduler/src/command/kind/clone_impl.rs
fac92a0b016f2b63e7383e57cc3b7c5eb03d58969424325451d34e9b58b39ade  crates/orchestration/peritus-scheduler/src/reducer/fences.rs
f1861bbce73ff9f0c21b6a5981e4a671363b01077e0e3a0e3a30cffec4233a4c  crates/orchestration/peritus-scheduler/src/reducer.rs
8d4639285927205a4ebf805cfd7bb805883a29f9fed6ebd76d3e95bd72591d51  crates/orchestration/peritus-scheduler/tests/worker_loss_outcomes.rs
cd148daf27ba86a3605b144c1d7c162e9f8dbaa6cd9806a95187d9b9fba9aaf3  crates/orchestration/peritus-scheduler/tests/command_fences.rs
```

## Remaining acceptance gaps

1. **Known queue-pressure defect blocks whole-package/GAP03 completion.** Both histories in `tests/queue_recovery.rs` are accepted and replayable, but a retryable failure or retry-safe worker loss can requeue active recoverable work after another item has consumed the last queue slot. The resulting two queued records under a configured limit of one fail checkpoint validation. This is a real transition-completeness/compatibility decision; it was not changed in this increment and must not be reported as a clean package suite.
2. **The loss proof does not yet compose the full transition.** The proof establishes exact selection and exact per-record classification. There is no single theorem or postcondition over `lose_worker` showing, for every valid pre-state, that every selected dispatch is released once to the classified phase/terminal, unrelated work and ownership are preserved, the worker becomes lost, the emitted outcome sequence equals the pre-state projection, and all successor/replay/checkpoint invariants hold across every possible release/error path. The regression test demonstrates one rich concrete family, not the universal composition claim.
3. **Release/package qualification remains outside this review.** The report binds only the listed frozen files and evidence. It does not establish workspace-wide tests, release tooling, or downstream compatibility.
