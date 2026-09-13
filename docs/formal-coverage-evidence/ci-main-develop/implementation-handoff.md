# Main/develop authority-custody checkpoint

Frozen at 2026-09-12T22:07:44Z from worktree `/home/doll/Project-Peritus/.worktrees/formal-coverage`, base/HEAD `1aa9282ff2cfcbf8fac325f11f157f408d33ee30`. No commit, push, pull request, live workflow run, GitHub settings change, App deployment, or ruleset change was performed.

## Implemented boundary

The direct `pull_request_target` workflow admits only PR bases `main` and `develop`. It keeps three distinct immutable revisions:

- `CHECKER_SHA = github.workflow_sha`, with `EVENT_SHA == CHECKER_SHA`, `EVENT_REF == refs/heads/main`, exact workflow ref on `refs/heads/main`, and default branch `main`;
- `BASE_SHA = github.event.pull_request.base.sha`, with exact target repository name and numeric repository ID and base ref `main` or `develop`;
- `CANDIDATE_SHA = github.event.pull_request.head.sha`.

The concurrency key contains all three revisions. Three credential-free depth-zero checkouts verify exact heads. The base must be an ancestor of the candidate. A main-base run additionally requires `BASE_SHA == CHECKER_SHA`; a develop-base run permits distinct commits while retaining exact protected-input equality.

Before Rust installation, Cargo, metadata, or candidate evaluation, all three Git trees must contain only regular blobs and must not contain legacy `.cargo/config` or `rust-toolchain` selectors. Both `CHECKER_SHA -> BASE_SHA` and `BASE_SHA -> CANDIDATE_SHA` must preserve modes and bytes for:

- `.cargo/config.toml`, `.gitattributes`, `rust-toolchain.toml`;
- `Cargo.toml`, `Cargo.lock`;
- `xtask/Cargo.toml`, reserved-absent `xtask/build.rs`, and all of `xtask/src`;
- `.github/workflows/formal-authority.yml`, `.github/workflows/formal-governance.yml`, and `docs/formal-governance-ruleset.template.json`.

The current `xtask/Cargo.toml` has no local/path dependency and no build script; dependencies are inherited external workspace dependencies whose declarations and exact lock are protected. Rust compiler dep-info for the production xtask binary confirms that the only compiled repository inputs outside `xtask/src` are the two protected workflow files and the protected ruleset template. Raw dep-info inventory is `/tmp/peritus-sol-authority-develop-dep-inputs-20260912T001.log`.

The checker is built from the checker checkout. Candidate evaluation invokes only that binary, uses isolated/offline Cargo metadata, reconstructs PATH from runner-resolved Cargo/Git plus fixed system directories, and binds proof impact to the exact PR base. Candidate packages, build scripts, procedural macros, actions, and repository scripts are not executed by this workflow.

The local positive bootstrap fixture establishes only that an already-installed authority version remains admissible when both subsequent custody edges are unchanged. Checker/dependency transitions still require independently reviewed external exact-head bootstrap on main followed by identical protected inputs on develop. This repository change does not provide that external authorization mechanism. The ordinary Actions result is not exclusive authority; no source-bound App, App secret, or live ruleset change exists here. Zero mandatory GitHub approvals and maintainer self-merge remain unchanged.

## Exact qualification commands

All commands ran from `/home/doll/Project-Peritus/.worktrees/formal-coverage`.

Focused defensive tests:

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/sol-authority-develop cargo +1.97.1 test --locked --package xtask workflow_authority > /tmp/peritus-sol-authority-develop-tests-20260912T002.log 2>&1
```

Result: PASS, 10 passed, 0 failed (plus filtered integration binaries).

Canonical reproducibility validation:

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/sol-authority-develop cargo +1.97.1 run --locked --package xtask -- reproducibility-check > /tmp/peritus-sol-authority-develop-reproducibility-20260912T002.log 2>&1
```

Result: PASS, 141 immutable action references.

Strict package Clippy:

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/sol-authority-develop cargo +1.97.1 clippy --locked --package xtask --all-targets -- -D warnings > /tmp/peritus-sol-authority-develop-clippy-20260912T002.log 2>&1
```

Result: PASS.

Package format check:

```sh
cargo +1.97.1 fmt --package xtask -- --check > /tmp/peritus-sol-authority-develop-fmt-check-20260912T002.log 2>&1
```

Result: PASS (empty log).

Workflow syntax/static check:

```sh
actionlint .github/workflows/formal-authority.yml > /tmp/peritus-sol-authority-develop-actionlint-20260912T002.log 2>&1
```

Result: PASS with actionlint 1.7.12 (empty validation log; version provenance in `/tmp/peritus-sol-authority-develop-actionlint-version-20260912T001.log`).

Whitespace check:

```sh
git diff --check > /tmp/peritus-sol-authority-develop-diff-check-20260912T002.log 2>&1
```

Result: PASS (empty log).

Full xtask package attempt:

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/sol-authority-develop cargo +1.97.1 test --locked --package xtask > /tmp/peritus-sol-authority-develop-package-tests-20260912T001.log 2>&1
```

Result: scoped unit suite PASS (427 passed, 0 failed, 1 ignored); CLI integration 2 passed and 1 failed solely because the concurrent scheduler source had `crates/orchestration/peritus-scheduler/src/work.rs` at 429 lines against the 400-line source-layout budget. This external integration failure is retained verbatim and was not suppressed or edited. The authority implementation and tests are each below 300 lines. Parent should rerun the full package/layout gate after the scheduler checkpoint is stable.

## Frozen source

Nine-file source manifest: `/tmp/peritus-sol-authority-develop-source-20260912T001.sha256` (SHA-256 `0598904df5514716922bf4fa9e62880ddf75c2851523ef6dc181d2ff39312f6d`). Exact source copy: `/tmp/peritus-sol-authority-develop-source-checkpoint-20260912T001`; its internal manifest is byte-identical.
