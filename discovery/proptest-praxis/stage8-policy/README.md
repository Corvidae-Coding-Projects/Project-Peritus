# Stage 8 policy and settlement property campaigns

These five public-interface suites run against local Peritus crates through a separate Cargo consumer. They do not alter production behavior or grant release authority.

Run from the repository root:

```sh
RUSTC_WRAPPER= CARGO_TARGET_DIR=target/stage8-policy PROPTEST_RNG_SEED=7319 \
  cargo test --offline --locked --manifest-path discovery/proptest-praxis/stage8-policy/Cargo.toml
RUSTC_WRAPPER= CARGO_TARGET_DIR=target/stage8-policy \
  cargo clippy --offline --locked --manifest-path discovery/proptest-praxis/stage8-policy/Cargo.toml --tests -- -D warnings
cargo fmt --manifest-path discovery/proptest-praxis/stage8-policy/Cargo.toml --all -- --check
```

At baseline `d106c9b13318fbcdd3d816ed63ae48532806fa7e`, the final source passed 5,120 generated histories and 109 fixed scenarios across five suites. The generated dimensions and independent expected results are visible in each test file:

| Suite | Actual boundary | Independent expectation |
| --- | --- | --- |
| `release_evidence.rs` | H4 `evaluate_release` | Classified artifact counts, contributing digest, conflict and order invariance for current/stale/mismatched/wrong-source/unreviewed/unsigned observations. |
| `release_reviews.rs` | H4 `evaluate_release` | Quorum, current reviewer independence, stale/mismatch/change counts and order invariance. |
| `quality_acceptance.rs` | B2 `evaluate_acceptance` | Gate-attempt cap, exact revision freshness, failed/missing gate and artifact truth table. |
| `gate_report.rs` | D1 `TargetGatePlan::discover` and `TargetGateReport` | Disposable Python project, required command multiset, honest optional observations, selected constraints and fail-closed outcome. |
| `cross_system.rs` | D1 public task ledger and qualification, gate report, candidate checkpoint, settlement | Exact hard-clause evidence is required alongside gate and review facts before a completed run can be accepted. |

Three compiled deliberate defects were caught by fixed examples and then reversed byte-for-byte: unsigned artifacts incorrectly contributed to H4 release evidence, a passing gate above the attempt cap ceased reporting `GateAttemptLimitExceeded`, and a missing command could satisfy D1 report coverage. The restored final source passed all suites, strict Clippy, and format checks.

No new production defect was confirmed in these bounded families. The cross-system test supplies synthetic gate execution records and review facts after real project-plan and public-clause construction; it is not a live provider, daemon restart, package installation, or publication test. Other Stage 8 lanes own analysis and telemetry campaigns. Native macOS/Windows and OS supervisor behavior remain outside this Linux-only consumer.
