# v0.1.14 defaults and installation: real-interface evidence

## Observable contract

Omitted and partial local-context configuration selects conservative decision views and
sequential read-only plans. Explicit history/disabled selections still win. Whole-memory
disablement and current child admission remain effective; separate candidate captures remain
the default. Exact results, continuation and recovery behavior are unchanged.

Source installation must enter before the initial Cargo compiler probe, bypass inherited
compiler wrappers for the installer process tree, and preserve real compilation/install
failures. Downloaded releases use native packages and require no Rust toolchain. Every native
release includes tag-bound Unix/Windows installers and checksums after six native jobs pass.

## Production interfaces exercised

The source is based on `b0d65b4914dd39e0ee3fabaffda91acf450460e8`, with the v0.1.14 defaults
and installer changes. External evidence is under
`/home/doll/.cache/peritus-efficiency-dif0njrm/defaults-release/`; generated state and provider
transcripts are not checked into Git. The current API consumer SHA-256 is
`40cb8a7ca9d19507130b29fd5de48b3dd92cf5b47b89c764865234cd57c1fa47`.

| Interface | Independent expectation and observed result |
| --- | --- |
| Public enum defaults, partial JSON and actual `DaemonConfig::load` TOML | Seven cases passed: omitted context, empty local config, unrelated partial config, each individual opt-out, both opt-outs, and whole-memory disabled. Candidate capture stayed separate. |
| Production decision/context APIs, omitted optimization settings | Eight groups passed, including host-bound invalidation, exact historical evidence and recovery. |
| Production inspection-plan APIs, omitted optimization settings | Nine groups passed, including dependent reads, exact results/continuations, current restrictions, restart and old-reader refusal. |
| Actual daemon IPC and native provider | BeforeEdit admitted the read-only continuation; full pause stopped before the next child. Both cases passed with no local policy section in the daemon TOML. |
| Official native Codex, omitted optimization settings | Two requests, actual model-authored search/read plan, observed decision-view compilation, exact independent answer and unchanged sources. 33,503 input tokens including 7,936 cached; 377 output; about 21.48 seconds. |
| Official OpenCode/Zen DeepSeek, omitted optimization settings | Two requests, actual search/read plan, observed decision compilation, exact answer and unchanged sources. 17,490 input including 7,040 cached; 549 output including 299 reasoning; about 9.34 seconds. |
| Actual managed CSV task with default policies | Accepted after real writing, checks and independent review; exact expected CSV, original HEAD/index unchanged, checkpoint/request binding verified. Seven requests, 148,233 input, zero cached, 1,053 output; about 67.95 seconds. |

These are behavior and wiring checks, not a new paired speed comparison. The previous I0–I6
reports retain the mixed, workload-specific measurements. No universal latency/token saving
is claimed. No unit/mock/formal/repository test suites supplied this evidence.

## Installation boundary and regression

A non-executable `sccache` reproduces the reported Cargo error exactly:
`could not execute process ... rustc -vV (never executed)` with `Permission denied (os error 13)`.
It occurs before xtask starts. Both CLI build-wrapper overrides and forced Cargo child-env
configuration fail to change an already inherited `RUSTC_WRAPPER` used by that initial probe.
Changing the Rust installer alone cannot intercept it.

The source installer therefore has an uncompiled Unix/PowerShell entry point. Unix exports
empty wrapper values before invoking Cargo; Windows removes inherited wrapper selections and
supplies an explicit temporary Cargo configuration. The Rust install command also clears
wrappers for its nested package build. Overrides stay in the install process tree and no user
compiler/cache configuration is modified. The existing source-install entry points select the
bootstrap. A direct outer `cargo xtask` launched with a broken `RUSTC_WRAPPER` remains subject
to Cargo's own pre-execution failure; this patch cannot change an external Cargo executable.

The Linux source replay mounts a fresh writable home inside bubblewrap without changing HOME,
uses the real checkout, toolchain, shared build target and native installer, and passes four
wrapper variables pointing at the non-executable fixture. It checks actual installation and
installed version independently. This reproduces the Linux execution-permission boundary;
it is not execution on the reporter's Windows/WSL host. The first mounted-home attempt
reached Chromium setup but failed because the fixture made `/tmp` read-only; the corrected
fixture supplies private writable `/tmp` and retains the original failure evidence. Windows native release/startup evidence
is tracked separately from the Unix source replay.

The release adds an active native staging job after all six package jobs. It requires all
48 native archive/evidence assets, then renders and uploads both version-bound bootstrap
scripts and their checksums (52 assets total), leaving publication separate. The full
Debian/RPM staging contract is preserved. Native install/startup, archive hashes, source/lock
binding and signed attestations are verified for the published tag separately.


## Build and static checks

Affected all-target/all-feature strict Clippy passed for product-runner, daemon and xtask.
The complete workspace all-target/all-feature check, formatting, shell/Python syntax and
selected diff checks are retained with the release evidence. Workflow syntax passed with
the existing deliberately disabled legacy jobs excluded from the constant-false diagnostic;
their disabled state was preserved. All 81 local lockfile packages are v0.1.14, and changed
manifests contain version-only edits.


The corrected Linux source replay passed end to end: browser setup completed, the installed
CLI reported `peritus 0.1.14`, and all three installed product binaries matched the package
hashes. A separate missing-real-compiler replay failed explicitly before installation, proving
that cache bypass does not convert ordinary compiler failure into success. The complete
workspace check passed. Release-native observations are attached to the exact-tag CI run.
