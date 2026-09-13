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
