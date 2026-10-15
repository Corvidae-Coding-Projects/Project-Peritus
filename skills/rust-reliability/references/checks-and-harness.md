# Executable checks and harness integration

## Runner scope

`scripts/rust_checks.py` runs real Cargo commands, records their outputs, and creates a lexical inventory of relevant source patterns. It does not run unit tests or operate application behavior. Python 3.9+ is required; Cargo, rustc, and the selected toolchain's Clippy component are required for the static checks.

Use a new evidence directory for each run; existing paths are refused to preserve earlier evidence. Run from any directory with absolute paths:

```bash
python3 /installed/rust-reliability/scripts/rust_checks.py \
  --manifest /project/Cargo.toml \
  --output /evidence/rust-checks \
  --package worker --features production --timeout 180
```

The helper uses `--locked` to avoid silently revising the dependency resolution. If the project has no lockfile, follow its normal dependency-resolution procedure first and document it. Do not suppress the failure or quietly rerun with relaxed rules. Use `--offline` when the required dependencies are cached and network access is unnecessary.

The default build and lint scope is Cargo's selected normal targets, not tests or every feature combination. Explicitly establish whether that scope covers the affected production path. Custom target selection and other project-specific commands must be run separately with equivalent recorded evidence.

The default Clippy command forbids:

- `unused_must_use`
- `let_underscore_lock` (rustc, for standard-library locks)
- `clippy::let_underscore_future`
- `clippy::let_underscore_lock` (for additional supported lock types)

`--forbid-unsafe` also forbids `unsafe_code` in the crates being linted. Select it only for scopes that are intended to contain no unsafe code. It does not audit dependencies or prove the absence of unsound safe abstractions.

An unsupported required lint yields **BLOCKED**. Compiler/build errors and known diagnostic failures yield **FAIL**. A timeout yields **BLOCKED** for the check; investigate hangs separately when the application behavior promises a deadline. Static-check success leaves behavioral validation **NOT RUN**.

The inventory deliberately contains only file/line locations and risk categories, not source contents. It searches lexical patterns and includes strings/comments; aliases, macros, qualified types, or wrappers can evade it. It is neither a Rust parser nor an acceptance gate. Inspect findings and relevant callers; do not claim safety from an empty result.

The source fingerprint covers regular files under the Cargo workspace root, excluding common generated/cache directories and the selected evidence directory. It includes paths and bytes. It excludes symlinks and does not hash external dependencies or tool binaries. A source change during the checks prevents static acceptance until the checks are rerun on stable inputs. Use a trusted harness's artifact/dependency/configuration identity as the authoritative provenance.

## Project-specific policy

Place policy in the project's established configuration with the user's existing authorization. Do not overwrite existing Clippy configuration or add blanket restrictions during an unrelated narrow fix.

For modules which must use a supervised worker API, consider disallowing direct abort APIs. For example, a project can configure `disallowed-methods` entries for `tokio::task::JoinHandle::abort` and `tokio::task::JoinSet::abort_all`, with reasons directing callers to the approved supervisor. Enforce `clippy::disallowed_methods` in the actual lint invocation. Verify paths and support against the pinned toolchain; do not enable `allow-invalid` for required rules.

Use the same approach for project-specific disallowed types or macros when a safe wrapper exists. Keep necessary infrastructure exceptions scoped and reviewed. Do not globally prohibit every timeout, select, clone, Arc, mutex, or unwrap. Those constructs are context-dependent.

Compiler `forbid` flags constrain source-level suppression of those lints. Cargo environment/configuration can still affect execution and lint levels; compiler wrappers, `--cap-lints`, changed runner scripts, or altered invocations can undermine enforcement. This helper is not an anti-tampering security boundary. A harness must control the runner, policy, environment, selected command scope, and compiler execution independently of the candidate.

## Generic harness boundary

Keep Rust expertise in this skill. Give the harness a language-independent validation protocol:

1. Associate a requirement set and required scenarios with a candidate identity before execution.
2. Execute checks through a trusted runner in an isolated environment. Keep the runner, policy, requirements, and expected results outside candidate write access.
3. Record commands/actions, executable/configuration identity, exit codes, actual observations, downstream readback, exploration budget, and concrete failure replays.
4. Inspect process evidence and artifacts. Do not accept a model-authored PASS field as execution provenance.
5. Require every applicable mandatory scenario to pass on the final build. Treat mandatory FAIL, BLOCKED, and NOT RUN as unresolved. Distinguish justified non-applicability from unexecuted work.
6. Invalidate relevant evidence when the candidate, configuration, toolchain, or acceptance contract changes.

The runner's JSON can seed static-check metadata. It does not implement this entire protocol and cannot automatically supply a behavioral oracle. No Peritus source changes are included in this skill package.

## Primary tool documentation

- Cargo build: https://doc.rust-lang.org/cargo/commands/cargo-build.html
- Cargo metadata: https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
- Clippy usage: https://doc.rust-lang.org/clippy/usage.html
- Clippy lint configuration: https://doc.rust-lang.org/clippy/configuration.html
- Clippy lint reference: https://rust-lang.github.io/rust-clippy/master/index.html
- Rust lint levels: https://doc.rust-lang.org/rustc/lints/levels.html
