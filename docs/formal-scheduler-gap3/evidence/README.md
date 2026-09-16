# GAP-03 checkpoint evidence

This directory retains selected unmodified local output and source identities for the bounded
checkpoint described in [the progress and remaining-gap record](../../formal-scheduler-gap3.md).
It is not a protected authorization record or proof that every hosted workflow passes.

## Source identity

`140-final-checkpoint-source.sha256` binds 1,196 files from the seven affected packages, `xtask`,
architecture, design, generated artifacts and fixtures. Its SHA-256 is
`6e2513f45819de038fdffb8775eafdff76e5152efd258023578ae7f92f011960`.
From the repository root, check the recorded source with:

```sh
sha256sum --check --quiet docs/formal-scheduler-gap3/evidence/140-final-checkpoint-source.sha256
```

The candidate was uncommitted when checks ran. The checkpoint commit preserves those same source
bytes. Documentation and review records are outside this source manifest to avoid self-reference.
The independent agent review checks the manifest as well as the implementation and tests.
Snapshots 132 and 136 retain the pre-lint and post-visibility-cleanup identities. Snapshot 140
also includes the formatter's final signature wrapping in the test-only cancellation oracle.

## Historical queue/version evidence

Logs 68 and 80-91 describe the earlier queue/version increment, before the cancellation and
module-layout changes. They are retained as historical evidence, not substituted for final checks.
The queue mutations in 80-83 intentionally fail verification and are followed by restoration and
the successful check in 84. Earlier failed queue/recovery reproduction and development output
remain in the local `target/gap3-evidence` directory.

`92-versioned-queue-source.sha256` and `92-versioned-queue-source.tar.gz` preserve the 980-file
snapshot covered by [the independent queue/version review](../queue-version-review.md). Extract
the archive into a separate directory to check that historical manifest. The decompressed tar
SHA-256 is `4c7542ace688efb4e4cb51c3588b885486c2d3224d7cef40ea885797c6e96334`.

## Final local results

| Output | Result |
|---|---|
| `128-final-checkpoint-verus.log` | Strict scheduler verification: 498 verified, zero errors, unchanged limits and no cheating |
| `141-final-checkpoint-tests.log` | Scheduler, protocol and projection: 109 passed, zero failed or ignored; includes 70 scheduler tests |
| `125b-final-daemon-tests.log` | Daemon library: 178 passed, zero failed, three existing ignored fixtures |
| `126-final-api-checker-tests.log` | API-checker regressions: 45 passed, zero failed or ignored |
| `142-final-checkpoint-clippy.log` | Strict Clippy passed for all targets/features of eight packages |
| `139-final-format-check.log` | Formatting passed |
| `120-final-codegen-check.log` | Generated protocol comparison passed |
| `130-final-ordinary-api.log` | 3,601 formal-boundary files and 14,755 ordinary-safe executable entry points passed |
| `134-final-architecture.log` | 84 packages and 4,514 source files passed architecture/layout checks |
| `135-final-reproducibility.log` | 141 immutable action references passed reproducibility checks |
| `133-final-protected-trust.log` | **Not qualified:** 49 stale and 38 missing proof-impact fingerprints; no trusted-construct finding |

Source changes after strict verification were confined to visibility/formatting in private
integration-test modules. Final ordinary tests and Clippy include those changes. The three daemon
ignores are the existing subprocess fixture (invoked by its parent test) and two graphical tests.
The trust output predates the last test-only signature wrapping; this does not resolve any of
its outstanding fingerprints. Final authorization and hosted workflows remain open.

## Cancellation command increment on draft PR 77

The next increment changes only the scheduler package and its evidence/documentation relative to
checkpoint `5020fddf692b0894c6c8d188efc3490a3419b8c8`. The 142-file scheduler manifest
`157-cancellation-source.sha256` has SHA-256
`ada577222b7b8587214ffd3a9580d224964dbf8efc7a56e442180b706b794def`.
Use the checkpoint commit for the unchanged remainder of the repository. Historical manifests
and reviews above remain bound to their original source, rather than being overwritten.
The [independent review](../cancellation-command-review.md) records the bounded result and its
reviewer identity; it is not a human approval or a protected authorization record.

| Output | Final increment result |
|---|---|
| `158-cancellation-final-verus.log` | 505 verified, zero errors; no cheating and unchanged resource limits |
| `159-cancellation-final-tests.log` | All 70 scheduler tests passed; zero failures or ignores |
| `156-cancellation-final-clippy.log` | All-targets, all-features scheduler Clippy passed with warnings denied |
| `160-cancellation-final-format.log` | Workspace formatting passed |
| `161-cancellation-final-architecture.log` | Architecture and source budgets passed |
| `162-cancellation-final-ordinary-api.log` | Ordinary API contracts passed |

The existing public cancellation test covers late completion before and after acknowledgement,
resource release, unrelated state, and replay/checkpoint roundtrips. Root admission and terminal
release now have exact production-called contracts; the remaining scheduler relationships and
final inventory/trust/hosted qualification stay open on the draft. This increment does not rerun
or relabel the historical daemon and API-checker test results as current scheduler tests.

## Work admission and cancellation capacity increment

Relative to checkpoint `39f56c636`, source manifest `182-admission-cancellation-source.sha256`
binds all 154 scheduler package files. Its SHA-256 is
`dae05abf83eea5d194cb3f6261ac4fcac64e9aa5cf6bb4dde178e49b429f205b`.
The [independent review](../admission-cancellation-review.md) covers the exact admission,
cancellation capacity/non-resurrection and individual worker-loss release claims. The root-to-chain
link, loss queue-bound/full-batch contract, whole reducer/replay and final trust/CI remain open.

| Output | Final increment result |
|---|---|
| `183-final-strict-verus.log` | 539 verified, zero errors; no cheating and unchanged resource limits |
| `184-final-scheduler-tests.log` | All 74 scheduler tests passed; zero failures or ignores |
| `176-scheduler-clippy.log` | All-targets, all-features scheduler Clippy passed with warnings denied |
| `181-final-format-check.log` | Workspace formatting passed |
| `185-final-architecture-check.log` | 84 packages and 4,532 source files passed |
| `186-final-ordinary-api-check.log` | 3,619 formal-boundary files and 14,761 ordinary-safe entry points passed |

The frozen manifest checked successfully after both final proof and test commands. Clippy preceded
only ghost-proof corrections and import formatting; the executable code was unchanged. Historical
source manifests and approvals are preserved. Raw output is copied byte-for-byte, including the
ordinary test log's trailing blank line.

## Worker-loss batch and replay reconstruction increment

Source manifest `211-composition-source.sha256` binds 168 scheduler paths relative to
checkpoint `c6db6c9e6`. Its SHA-256 is
`c7a5b16af1112d16e0cb859cd54ef325b85369976acde7b71944ae86cbd2ffc4`.
It checked successfully after the final ordinary tests, Clippy and restored strict proof run.
The [worker-loss review](../worker-loss-batch-review.md),
[cancellation/reconstruction review](../cancellation-reconstruction-review.md) and
[ordinary integration review](../reducer-integration-review.md) retain distinct review scopes
and explicitly exclude each reviewer's authored implementation.

| Output | Final increment result |
|---|---|
| `214-final-composition-verus.log` | 590 verified, zero errors; no cheating and unchanged resource limits |
| `212-final-composition-tests.log` | All 77 scheduler tests passed; zero failures or ignores |
| `213-final-composition-clippy.log` | All-targets, all-features scheduler Clippy passed with warnings denied |
| `208b-final-composition-format.log` | Workspace formatting passed |
| `209-final-composition-architecture.log` | 84 packages and 4,546 source files passed |
| `210-final-composition-api.log` | 3,633 formal-boundary files and 14,763 ordinary-safe entry points passed |
| `215-composition-docs-check.log` | 255 documentation files passed |

Negative probe `206-reconstruction-mutation.sh` reverses the cancellation-tree reconstruction
flag. Its retained diff and output show the exact payload postcondition fails (589 verified,
one failing verification item, exit 101). The restoration record confirms byte-identical source
restoration. The final strict proof run occurs after restoration. Public regressions require the
exact replay mismatch kind and detail for changed successor digests and derived cancellation
identities, and retain mixed-worker loss/resource/queue/replay coverage. Earlier output 204, 205
and 207 records the initial green combined source; 208 records a subsequently corrected test
line-wrap. Historical evidence is not overwritten.

This increment closes the bounded root-to-cancellation-chain link, loss queue/batch/worker/event
composition, and exact replay input/cursor/transition contracts. Whole reducer/replay/caller
composition, remaining command/refresh/selection/finalization paths, final source authorization
and hosted qualification remain open. No historical approval or obligation is relabelled.

## Dependency scanning, worker refresh and finalization increment

Manifest `228c-refresh-source.sha256` binds 176 scheduler paths relative to checkpoint
`8644a2d2b`, with SHA-256
`f3682e9867d5e449f36c6db291a58244716d9df99af38c926adb3531af596aab`.
The [refresh review](../refresh-review.md) and [finalization review](../finalization-review.md)
retain independent scopes, author exclusions, exact source identities and the remaining outer
dependency-loop and reducer/replay boundaries.

| Output | Final increment result |
|---|---|
| `230c-final-refresh-verus.log` | 628 verified, zero errors; no cheating and unchanged resource limits |
| `229b-final-refresh-tests.log` | All 79 scheduler tests passed; zero failures or ignores |
| `231b-final-refresh-clippy.log` | All-targets, all-features scheduler Clippy passed with warnings denied |
| `226c-refresh-format.log` | Workspace formatting passed |
| `227-refresh-architecture.log` | 84 packages and 4,554 source files passed |
| `232-refresh-api.log` | 3,641 formal-boundary files and 14,764 ordinary-safe entry points passed |
| `233-refresh-docs-check.log` | 257 documentation files passed |

Negative probe `225-worker-count-mutation.sh` inverts worker ownership in the iterative
reservation count. Its exact suffix-count invariant fails (627 verified, one failing item,
exit 101), and the restoration record confirms byte-identical restoration. The final strict
run follows restoration and the ghost-helper file split needed for the repository source budget.
Earlier output 220, 223 and 224 records the preceding ordinary/proof checks; 226 records the
subsequently corrected import ordering. The per-round dependency applicator and complete
fixed-point termination theorem are not part of this increment.

## Final dependency, selection and reducer completion

Manifest `257-final-scheduler-source.sha256` binds all 189 files in the scheduler package after
the dependency fixed point, exact selector and dispatch adapter, pending-directive relation, and
genesis/decision commitment seams were integrated. Its SHA-256 is
`fe6b85f563ed3cbc6ca990b3e6e6d041734eb2592da0ba4a23416b911bd67240`.

| Output | Final combined result |
|---|---|
| `251-final-combined-verus.log` | 693 verified, zero errors; no cheating and unchanged resource limit |
| `252-final-scheduler-tests.log` | All 79 scheduler tests passed; zero failures or ignores |
| `254-final-scheduler-clippy.log` | All-targets, all-features scheduler Clippy passed with warnings denied |
| `253-final-format-check.log` | Workspace formatting passed |
| `255-final-architecture-check.log` | 84 packages and 4,567 source files passed |
| `256-final-ordinary-api-check.log` | 3,654 formal-boundary files and 14,765 ordinary-safe entry points passed |
| `258-final-docs-check.log` | 257 documentation files passed |
| `259-ordinary-package-gates.log` | All 66 affected-package ordinary commands passed |
| `260-verus-package-gates.log` | All 66 affected-package Verus commands passed; V/H used `--no-cheating`, while the TCB used its declared trust-aware command |
| `261-final-docs-check.log` | Final documentation set passed with the review and proof-impact inventory linked |

The retained tests cover exact replay and tamper rejection, durable restart/session callers,
dispatch error precedence, dependency cascades, selection fairness, queue recovery and both wire
schema versions. The proof contracts keep SHA-256 execution and encoded-size evaluation explicit
ordinary boundaries. The [independent final-candidate review](../final-candidate-review.md) passes
the exact candidate commit and tree without claiming human or GitHub approval. Its SHA-256 is
`850feac54d2e92387d97331b5f6b29aa70c60cbec42a18362140f10de0cb6699`. The
[`pcr7-inventory`](pcr7-inventory/README.md) bundle binds the protected-base comparison, all
155 source transitions, all 66 affected packages and the exact 132-command plan. Its contents are
audit evidence, not protected-base authorization or obligation discharge. All 137 recorded
obligation statuses remain `in-progress`, and historical records above remain unchanged.

## Rerunnable local checks

Use the repository's pinned toolchains. These commands keep one build job; strict Verus uses
two CPUs and the original resource limit:

```sh
export CARGO_BUILD_JOBS=1 CCACHE_DISABLE=1 RUST_TEST_THREADS=1
taskset -c 0,1 cargo verus verify --package peritus-scheduler --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo test -p peritus-scheduler -p peritus-protocol -p peritus-projection --all-features --locked --no-fail-fast
cargo test -p peritus-daemon --lib --all-features --locked
cargo test -p xtask --locked api_contract::
cargo clippy -p peritus-scheduler -p peritus-protocol -p peritus-app-protocol -p peritus-projection -p peritus-evidence -p peritus-daemon -p peritus-performance-qualification -p xtask --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
cargo run --locked --package peritus-protocol --bin peritus-protocol-codegen -- --root . --check
cargo xtask architecture-check
cargo xtask ordinary-api-check
cargo xtask docs-check
cargo xtask reproducibility-check
```

Protected-base trust requires a separately authorized inventory matching the final source. It
must not be reported as passing merely because these local code checks pass. No historical
approval, obligation status, live branch rule or maintainer permission changes in this checkpoint.
