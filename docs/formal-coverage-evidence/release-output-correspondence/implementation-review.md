# Release-policy declarative readiness and output correspondence checkpoint

## Identity and frozen source

- Worktree: `/home/doll/Project-Peritus/.worktrees/formal-coverage`
- Branch: `feature/formal-coverage`
- HEAD/base: `1aa9282ff2cfcbf8fac325f11f157f408d33ee30`
- Freeze time: `2026-09-12T23:32:05Z`
- Frozen source: `/tmp/peritus-sol-release-output-source-20260912T001`
- Source file list: `/tmp/peritus-sol-release-output-source-files-20260912T001.txt` (55 files)
- Source manifest: `/tmp/peritus-sol-release-output-source-20260912T001.sha256`
- Source manifest SHA-256: `88634c8e6304cbf8c923a2912cef2386b1f8ebc265222560e9bbbe20ce955c86`
- Source list SHA-256: `849e5b5d5a8927f8414e897cfdb6f5ec658f70c9d9777340c9ded600c5758d7e`

The snapshot was copied after all qualification commands and independently checked with `sha256sum -c` for all 55 entries. No package manifest was changed. The shared `Cargo.lock` was already modified outside this ownership; the qualification-time hash recorded after the commands was `f15ad47b4bafe5a71deda4e5e11c075478f484c62572ce814ce07f2d73134205`.

## Bounded result

The production `evaluate` path now proves raw `ReleaseVerdict::Ready`, `ReleaseDecision::is_ready`, aggregate readiness, and empty canonical diagnostics exactly when `release_inputs_ready` holds for the supplied evidence, exact release candidate, and evaluation time.

That declarative predicate is connected to the actual finite production traversals. It includes the 44 evidence requirements, the fixed 25 criterion slots and values, artifact consistency, qualification observations and their fixed admission checks, reviewer observation validity/independence/quorum, and every finding/waiver/request observation and relevant pair relation. The finding reducer additionally retains the exact saturated counters and conflict flag produced by its three traversals.

The returned assessments preserve the production-computed fields: canonical requirement/criterion identities and values, evidence counts and XOR aggregate, qualification counts/conflict/report digest, review counts, finding and waiver counts/conflict, and diagnostics in exact production order. The decision digest contract models the exact byte sequence used by the production reducer and ties it to the candidate manifest digest, raw verdict, and the evidence, qualification, review, and finding assessments.

Constructor admission contracts cover the involved evidence, qualification, review, finding, waiver, and binding values, including exact success conditions and represented failure kind/precedence where the public error type distinguishes it. Tests exercise a complete admitted input and missing or conflicting dimensions, exact output slots/counts/digests, and binding error precedence.

## Qualification

Strict release-policy proof, fresh on-disk target:

```text
CARGO_BUILD_JOBS=2 CCACHE_DISABLE=1 CARGO_TARGET_DIR=target/sol-release-output-final-verus-002 cargo verus verify --package peritus-release-policy --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20 --multiple-errors 8
```

Result: `377 verified, 0 errors` in the release-policy root. Dependency roots also reported `2044 verified, 0 errors` and `210 verified, 0 errors`. Log: `/tmp/peritus-sol-release-output-verus-final-20260912T002.log`.

Focused four-package ordinary tests:

```text
CARGO_BUILD_JOBS=2 CCACHE_DISABLE=1 CARGO_TARGET_DIR=target/sol-readiness-four-package-final cargo test --package peritus-release-policy --package peritus-security-policy --package peritus-release-qualification --package peritus-security-qualification --all-features --locked
```

Result: 81 executable tests passed, 0 failed, plus 1 compile-fail documentation test passed. Package totals were release-policy 28, release-qualification 14, security-policy 7, and security-qualification 32. Log: `/tmp/peritus-sol-readiness-four-package-tests-final-20260912T001.log`.

Strict four-package Clippy:

```text
CARGO_BUILD_JOBS=2 CCACHE_DISABLE=1 CARGO_TARGET_DIR=target/sol-readiness-four-package-final cargo clippy --package peritus-release-policy --package peritus-security-policy --package peritus-release-qualification --package peritus-security-qualification --all-features --all-targets --locked -- -D warnings
```

Result: pass. Log: `/tmp/peritus-sol-readiness-four-package-clippy-final-20260912T001.log`.

Four-package format:

```text
cargo fmt --package peritus-release-policy --package peritus-security-policy --package peritus-release-qualification --package peritus-security-qualification -- --check
```

Result: pass. Log: `/tmp/peritus-sol-readiness-four-package-fmt-final-20260912T001.log`.

Repository ordinary API gate:

```text
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/sol-release-output-xtask cargo run --locked --package xtask -- ordinary-api-check
```

Result: pass, 3,492 formal-boundary files and 14,680 ordinary-safe executable entry points. Log: `/tmp/peritus-sol-release-output-ordinary-api-final-20260912T002.log`.

Repository source-layout gate:

```text
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/sol-release-output-xtask cargo run --locked --package xtask -- source-layout-check
```

Result: pass, 4,393 source files. The largest newly split production traversal is 399 lines. Log: `/tmp/peritus-sol-release-output-source-layout-final-20260912T002.log`.

`git diff --check -- crates/foundation/peritus-release-policy` passed. No `Err(_) => true` constructor escape was present. The final scan found no verifier external-body/assume specification annotations; ordinary `cfg(verus_only)` proof declarations remain part of the repository's established Verus integration.

## Proved, tested, and external boundaries

Proved here: deterministic reduction of the supplied in-memory policy inputs, exact finite slot/order/value relationships described above, raw verdict/readiness equivalence, exact retained reducer fields, exact canonical diagnostic sequence, and exact decision fingerprint algorithm.

Tested here: complete admission and meaningful missing, stale, mismatched, conflicting, quorum, waiver, and constructor failure cases across the four production packages.

External or intentionally open: authenticity and truth of supplied observations; native command execution; filesystem, network, signer, and provider I/O; cryptographic collision resistance or signature authenticity; publication/finalization authority; and deployment of this source. A nonzero digest constructor proves only the declared deterministic admission rule. The decision digest is a deterministic policy fingerprint, not an authenticated signature. Its byte model follows the current production inputs; criteria are determined from evidence during evaluation and are not separately appended to that digest. This checkpoint does not alter H4 draft staging or its separate manual publication finalization.

The proof establishes correspondence for the actual supplied collections and their production traversal order. It does not prove that external systems produced truthful observations, nor does it authorize a release. No whole-goal completion or review approval is claimed.
