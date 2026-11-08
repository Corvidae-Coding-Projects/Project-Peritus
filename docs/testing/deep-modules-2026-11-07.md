# Deep modules and interface simplification — November 7, 2026

Verdict: **ready** for the three implemented boundaries. Source is the working diff on
`develop` above `fb38b4c189241b4e2be53a95c01a32b8d964d621`; this is not a release or
installation record. The [design and coverage record](../../.design/deep-modules-and-interfaces.md)
identifies inspected owners, migrated consumers, unchanged areas and rollback.

## Observable contracts and results

| Boundary | Required behavior | Observed result |
| --- | --- | --- |
| Prepared model context | Prepare the exact selected model, preserve route/capability identity, and use one numeric snapshot for input projection and output fitting. | PASS: saved snapshot remains unchanged after metadata changes; another model is rejected; configured snapshots retain their limits; zero/exhausted reply allowances fail. |
| Actual model metadata | Use the selected runtime model's advertised capacity through the new public API. | PASS: native Codex discovery for `gpt-6.1-sol` returned input 828,144, output 32,000, with 256 reserved for the reply; configured bootstrap input was 200,000. Metadata only, no inference sent. |
| Manifest-bound stages | Resolve the next role from the job's package; expose only declared predecessor reads; publish the exact bound stage and preserve recovery. | PASS on real files: exclusive job ownership, abandoned stage, empty-output rejection, undeclared committed-input rejection, UTF-8 boundaries/continuation, publication and reopen. |
| Existing job inspection | Reject corrupt predecessors even when the final artifact is intact; recover after restoration. | PASS through the existing public inspection consumer. |
| Model menu | Agent → model → effort; no intermediate save; Back/cancel retain prior choices; final save preserves sibling roles; `/effort` remains independent. | PASS in a real PTY client connected to the local daemon and in protocol/key/render replays. The active coding run's role settings were unchanged. |
| Asynchronous menu boundaries | Keep the selected identifier across reordered catalogs, including a refresh arriving on the effort page; reject changed destinations, stale responses and removed models. | PASS in retained request-correlation/key-handler regressions; all three roles exercised. |

No new randomized campaign was run. Existing property campaigns execute within the focused
library suites. The new cases are deterministic boundary replays, not a claim of exhaustive
state-space coverage or a performance measurement.

## Commands and evidence

Commands ran on Linux x86-64 with the pinned repository toolchain and two Cargo workers:

```sh
CARGO_BUILD_JOBS=2 cargo check --locked --workspace --all-targets
CARGO_BUILD_JOBS=2 cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-provider-core -p peritus-provider-openai -p peritus-product-runner -p peritus-launcher -p peritus-tui --lib
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-tui --lib
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-modules --example pending_stage
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-modules --example inspect_predecessor_integrity
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-provider-openai --all-features --example context_capacity -- gpt-6.1-sol --metadata-only
cargo fmt --all -- --check
git diff --check
```

Workspace checking and final strict Clippy passed for all 81 active packages. Final focused
library results total 120 passing tests: provider core 4, OpenAI 4, product runner 91,
launcher 3, TUI 18. An initial new test incorrectly used End to request XHigh; the public
effort list ends at Ultra. The driver now explicitly navigates to XHigh. No production
behavior was weakened to satisfy that expectation.

Strict Clippy also found preexisting unseparated integer literals in the argument-admission
example. Separators were added without changing their values or suppressing lints.

The ordinary [capacity consumer](reproducers/model-capacities/README.md) was linked against
the final production Cargo artifact cohort and its `checks` command passed: ten independent
numeric fixtures, four invalid-metadata cases, seven shared-window cases and configured
snapshot preservation. It and its recorder now use the prepared API. Its older one-token
uncapped reservation expectations were updated to the already established current contract
that reserves the advertised reply maximum for an uncapped caller. Its linker was repaired
for dependencies required by the retained shared support source.

OAuth and three copied-state host consumers were also type-checked against that cohort.
Their old missing optional fields were supplied explicitly; copied-state replays retain their
recorded profile instead of making live discovery part of historical assembly. Those four
consumers were not behaviorally rerun against accounts or historical user-state copies.

The final native terminal consumer calls production `peritus_tui::run`/`TuiConfig` and connects
to the existing daemon with an unstarted disposable conversation. It uses actual arrow keys,
Enter and Escape, decodes terminal output and checks settings readback. A debug-build replay
initially observed a partially painted frame; its wait condition was corrected to require
the complete expected rows before asserting. The final replay passed. Its executable SHA-256:
`a80fd9bb38a5c3f9d1fab8dd45101cc6bd6322e530cc7e96f4ae868f5d9835c0`.

Local logs and native screen captures are under `/home/doll/.cache/` with prefix
`peritus-deep-modules-`; the source inventory is
`peritus-interface-survey-20261107.json`. They contain the actual commands/results and are
local evidence, not portable fixtures or checked-in successful-run transcripts.

## Limits

This pass establishes the three changed interfaces and their inspected callers. Cargo
inventory and successful compilation do not establish whole-source correctness. Provider
inference conformance, native executable plugin behavior, browser journeys, Windows/macOS
execution and unrelated persistence/runtime internals were not requalified. The running
daemon was neither replaced nor interrupted; its authority and active run remain separate
from the newly built source consumers.
