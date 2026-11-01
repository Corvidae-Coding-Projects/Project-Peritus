# Property-suite strengthening

This review starts from `a4e225a99f6a81ab4c687e0e503a067b242a778c` (v0.1.18).
It reviews the existing property machinery and strengthens targeted missing interactions;
it does not assert whole-product or all-platform correctness. Production remains the
subject of the tests. No real provider account, user installation, or release is modified.

## Main changes

- Construct related valid inputs and one-defect variants so rejected setup or unrelated
  rejection cannot satisfy the intended property.
- Exercise history: output replay and clone isolation, graph edge replacement, competing
  memories, completed module predecessors, live lease binding checks, interrupted file
  scans, and recovery after rejected operations.
- Separate observation from the model. The clustering oracle now requires each returned
  group to be a complete equivalence class and preserves every member's fields. It no
  longer silently merges split output groups itself or calls the production classifier.
- Check Unicode byte boundaries across all five TUI editor modes, including negotiated
  prompt/signature limits, cursor repositioning, and continued use after rejection. The
  reference editor uses a scalar vector while the real method uses byte-indexed strings.
- Selection tests use ordered objective lists, deliberate ties, failed and unavailable
  criteria, and canonical identity rejection. Statistics use a wider sum model around
  `u64::MAX` and an independent cumulative-frequency percentile oracle.
- Telemetry checkpoint histories now vary every acknowledgement identity field. A fixed
  sequence rejects each altered acknowledgement, retains the exact batch, and retries it.
- Account executable discovery exhausts all pairs of seven PATH entry types, including
  directories and valid, invalid, or dangling symlinks, with Unicode/spaced paths and
  generated longer search paths. Failure reduction preserves the discovered mismatch.
- Public and owner controllers require matching behavioral reports and native process
  outcomes. Missing, malformed, non-object, unknown, contradictory, signaled, and timed-out
  results cannot pass. Setup-incomplete cases abort generated search instead of being
  discarded. Local package lock entries are reconciled with v0.1.18 without changing
  third-party dependency versions.
- Daemon fixtures now distinguish a tool-free public steering reply from the resumed task
  and verify both turns, exact selected model/effort, queued input, and the visible reply.
  Unset effort must pause without inference; explicitly selecting an effort resumes work.
  These correct outdated test schedules without changing the runtime's selection policy.
- Failure persistence works in the integration-only policy workspace, and statistics
  regressions resolve relative to their package rather than the caller's working directory.

## Coverage record

The state, provider, and policy contract notes record target-specific audit depth and
remaining limits. These are reviews of generators, oracles, and their production
boundaries, not line-coverage measurements.

| Area | Review and scope |
| --- | --- |
| State/context Stage 1–4 | [State review](contracts/state-strengthening.md): 19 assigned targets, with deeper changes to graph, memory, review restoration, and module histories. |
| Framing, model/schema, provider Stage 5 | [Framing](contracts/framing.md), [models](contracts/stage5-models.md), [tools](contracts/stage5-tools.md), [providers](contracts/stage5-providers.md): valid/deep structures, replay histories, exact error paths and outbound request content. Codec/wire/MCP/plugin limitations remain in their own contracts. |
| Native/policy Stage 6 and Stage 8 | [Policy review](contracts/strengthening-policy.md): all twelve Stage 6 targets and five policy suites, with live probes and positive controls where the old generators missed them. |
| Public telemetry | Reviewed queue/disposition model, operation weights, capacity-reaching retry patterns, and all acknowledgement defect variants. Existing public campaign already covers all ack fields; the checkpoint suite now does too. |
| Owner reads/decision/plans/inputs | Reviewed operation strategies, byte-visibility assertions, current-input accounting, durable plan boundaries, and controller/case plumbing. Existing rich models retained. Pure input capture is distinct from daemon admission. Not a fresh line-by-line audit of every owner implementation. |
| Daemon/context flow | Reviewed actual IPC schedule wiring, steering/cancellation/restart fixtures, and archive→decision→plan receipt lineage. [Daemon schedules](contracts/stage3-daemon.md) now assert public replies and resumed work independently. Provider dependencies remain controlled fixtures; no live inference claim. |
| TUI/WebUI/launcher | TUI source-mounted insertion strengthened; full mouse/keyboard routing remains outside it. Existing real TypeScript workspace scheduler checks four recovery paths and stale project responses. Launcher executable discovery strengthened; release parsing remains a separate bounded consumer. |
| Analysis/telemetry package tests | Clustering, multi-objective selection, wide statistics, and checkpoint acknowledgement histories strengthened and run through the real Rust owners/public APIs. |
| Controllers | Disposable real child processes exercise both controllers' report/process classification. No fabricated product behavior is involved in this harness self-test. |

## Calibration

A deliberate defect must compile, pass the old weak check where claimed, and fail the
strengthened check for the expected mismatch. Every mutation is restored before final
validation. Calibration failures are not production findings.

- Disabling clustering entirely passed the old generated property. The strengthened
  property rejected two equivalent members left in separate groups.
- Ignoring telemetry acknowledgement stream identity passed the old checkpoint-history
  property (which varied only count); the new exact field/retry case rejected it. The
  separate public telemetry campaign already covered this field.
- Policy calibration checks include erased review binding sequence, unconditional
  acceptance rejection, and a constant socket identity hash; see the policy review.
- Provider calibration removes aggregate SSE accounting, clears mutable items on a
  retransmission, and validates only the first array item. Each fails the corresponding
  new generated family and shrinks to an exact witness.
- State calibration preserves old graph links during replacement and omits same-cycle
  restoration precedence. Both fixed and generated checks detect each defect.

Ten compiled mutations were detected across these lanes and restored byte-for-byte.
The module predecessor defect is a separate real production finding: a two-stage job
with same-size damage in stage zero was reported as fully completed by `Job::inspect`.
The fix verifies predecessors before returning the latest output page. Its standalone
consumer fails before the fix, passes after it, and verifies recovery after restoring
exact bytes while preserving the checkpoint and active ownership.

## Validation and replay

Run artifacts, source fingerprints, exact cases, and logs for this pass are local under
`/home/doll/.cache/peritus-praxis-strengthen/`. Use a new output path for each invocation.

The following completed checks cover the integrated sweep and focused investigations:

| Check | Observed result | Local evidence |
| --- | --- | --- |
| Integrated public examples | All 42 targets passed, totaling 1,206 fixed cases against the composed changes. | `integrated-examples/summary.json` |
| Integrated public search | All 42 targets passed 256 histories each: 10,752 generated histories, seed `412679`, requested history bound 96 (target-specific caps apply). No timeout or incomplete target. | `integrated-search/summary.json` |
| Strengthened state families | 4,096 generated histories over two seeds and 62 fixed cases passed; both calibration defects detected and restored. | `state-*`, `state-calibration/report.json` |
| Provider/framing/schema families | 15,360 generated framing/model/schema cases over three seeds, 256 public provider cases, 256 private OpenAI cases, 483 public fixed cases and 15 private fixed cases passed. Three calibration defects detected and restored. | `provider-runs/` |
| Strengthened native/policy families | 6,144 generated native-contract histories over two seeds passed. The five policy suites passed 9,216 generated cases and 203 fixed scenarios per seed, including the integrated seed `412679`. Three calibration defects detected and restored. | `policy-runs/`, `integrated-policy.log` |
| Analysis/checkpoint suites | 4,096 generated histories per seed (`984731`, `412679`) plus fixed regressions passed. | `analysis-tests.log`, `analysis-final-tests.log` |
| Extended TUI insertion | 4,096 histories passed: 33,848 pastes, 34,175 typed characters, 19,689 cursor moves, 13,164 oversize rejections and 7,002 successful operations after rejection. Every editor reached its cap and rejection paths. | `tui-extended/tui_paste/case-output/report.json` |
| Owner reads/decision/plans/inputs | 103 fixed scenarios and 512 generated histories passed. | `owner-examples/summary.json`, `owner-search/summary.json` |
| Context-flow integration | 128 generated histories and two fixed scenarios passed through the actual product-runner owner. | `context-flow.log` |
| Daemon schedules | Nine fixed scenarios and 64 generated histories (seed `32189`) passed with 81 public-reply/resumption checks, 12 unset-effort pauses without inference, 11 crash recoveries, and four retained command-effect recoveries. | `policy-runs/daemon-examples5/summary.json`, `policy-runs/daemon-search64/summary.json` |
| Integrated daemon verification | The final checkout passed all nine fixed scenarios and another 32 generated histories with fresh seed `412679`. | `integrated-daemon-examples/summary.json`, `integrated-daemon-search/summary.json` |
| Statistics persistence correction | All six statistics tests passed after correcting the regression path: 2,048 generated cases and two fixed tests. | `eval-persistence-final.log` |
| Controllers | All 30 process/report classifications passed. The original public controller failed 11 of its 15 scenarios. | `controller-before-run/report.json`, `controller-after-run/report.json` |
| Launcher discovery/release parsing | 620 executable-discovery cases and 1,037 release-parser cases passed. | `provider-discovery/report.json`, `release-parser/report.json` |
| WebUI | 518 drafts through four recovery paths, the stale-project schedule, and 513 decimal sequence checks passed against the real TypeScript module. | `web-workspace.log` |
| Linux install scripts | 99 lifecycle and 71 bootstrap scenarios passed in disposable homes with synthetic package binaries and shim dependencies. | `linux-lifecycle/report.json`, `linux-bootstrap/report.json` |
| CLI web grammar | 16 malformed-command cases passed against the installed v0.1.18 executable; CLI source is unchanged. | `cli-grammar/report.json` |
| Module repair | Standalone predecessor-corruption regression failed before the fix and passed after it, including restoration/reopen. | `state-regression-before.log`, `state-regression-after.log`, `module-regression.log` |
| Build and static checks | Locked offline public/owner builds, strict Clippy for the public harness, policy suites, affected analysis/telemetry/module packages and final daemon owner passed. Formatting passed for all three workspaces; final diff whitespace check passed. | `integrated-build/`, `integrated-public-lint/`, `root-lint.log`, `policy-runs/`, `integrated-daemon-lint/`, `*-fmt.log` |

The public sweep includes the module repair and all public generator/oracle enhancements.
The final daemon fixture changes and statistics persistence-path correction were applied
after that sweep; they receive their own owner/package checks rather than repeating
unaffected public targets. All fixes and test changes are integrated into this checkout.

These are bounded local runs. Controlled provider fixtures do not establish live-account
inference; source-mounted TUI insertion does not establish terminal mouse/keyboard routing;
the native contract models do not establish OS sandbox conformance. Platform-specific
checks here ran on Linux. Case totals count generated histories, not distinct defects or
exhaustive state-space coverage.

Representative reruns from the repository root (choose fresh output directories):

```sh
python3 discovery/proptest-praxis/campaign.py build --output /tmp/praxis-strengthened-build
python3 discovery/proptest-praxis/campaign.py examples --target all --output /tmp/praxis-strengthened-examples
python3 discovery/proptest-praxis/campaign.py search --target all --cases 256 --max-steps 96 --seed 412679 --timeout 300 --output /tmp/praxis-strengthened-search
python3 discovery/proptest-praxis/controller_checks.py --output /tmp/praxis-strengthened-controllers
RUSTC_WRAPPER= CARGO_PROFILE_DEV_DEBUG=0 PROPTEST_RNG_SEED=412679 \
  cargo test --locked --offline -p peritus-debugger -p peritus-eval -p peritus-evolution -p peritus-telemetry --lib --tests
RUSTC_WRAPPER= CARGO_PROFILE_DEV_DEBUG=0 PROPTEST_RNG_SEED=412679 \
  cargo test --locked --offline --manifest-path discovery/proptest-praxis/stage8-policy/Cargo.toml
RUSTC_WRAPPER= CARGO_PROFILE_DEV_DEBUG=0 \
  cargo run --locked --offline -p peritus-modules --example inspect_predecessor_integrity
```

For owner consumers, build using `owner.py build --target TARGET`, then pass its fresh
`receipt.json` to `owner.py examples` or `owner.py search`. Receipts bind source and
executable digests; rebuild after changing either. Target-specific fixtures and remaining
limits are documented in the contract links above.
