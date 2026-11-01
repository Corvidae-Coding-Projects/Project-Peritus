# Policy and native-boundary property review

The review traced the generator, checker, and observable interface for all twelve Stage 6 target modules and all five Stage 8 policy suites. This is a bounded suite review, not whole-product qualification.

| Suite | Assessment and resulting work |
| --- | --- |
| `authority` | Independent wider budget arithmetic and explicit lifecycle oracle are useful. One account and mostly one budget dimension remain; hierarchical reclamation is outside this harness. |
| `kernel` | Checks rejected-state preservation and event bindings against an explicit phase model. Identity reuse means only one run/attempt/turn is explored; additional fresh identities and acceptance paths remain separate work. |
| `router` | Real committed authority and dispatch counter catch denied/replayed effects. The fixture is a terminal non-idempotent tool; active-tool cancellation and concurrent dispatch are not covered. |
| `filesystem` | Real patches, canaries, completion failure and recovery preserve independent file-tree expectations. Mid-mutation crash schedules are not injected by this harness. |
| `fs_snapshot` | Real Git snapshot paging checks bytes, encoding, digests, continuation and writer isolation. Small single-file fixtures do not cover rename trees or filesystem races. |
| `fs_dispatch` | Real router/adapter verifies exact page payload and idempotent replay after writer divergence. This is one read call, not cancellation/concurrency qualification. |
| `scoped_read` | Added real cancellation after the first callback, same-size source rewrites, pathname replacement during scans, and fresh readback after rejection. Generated sizes straddle the 64 KiB scan boundary. |
| `host` | Random lease histories often became terminal before checking bindings. Added guaranteed live rejection probes for every binding, reference resource identity, and changed delivery types, followed by exact use/exhaustion. Added distinct socket identity checks for both parent and final components; stability alone could accept a constant hash. |
| `sandbox` | Per-bit IP oracle is independent, but adjacent queries previously flipped only the last address bit. Added inside/outside prefix-bit probes and ports immediately around each endpoint. These are pure contract decisions, not OS sandbox conformance. |
| `process_output` | Added constructive live lifecycle sequences with competing stop reasons, decreasing sequence numbers, spawn failure and repeated terminal transitions. Public search checks lifecycle/policy; actual output capture remains the separate native-process executable. |
| `spec` | Small graph scheduling includes exhaustive enumeration through six nodes, plus larger bitset models and malformed declarations. Identifier distributions remain compact and canonical. |
| `security` | Full positive evidence fixture and explicit expected diagnostics cover combined families; one fault per evidence class and one changed candidate digest remain limits. |
| `release_evidence` | Counts, digest, conflict and permutation checks are meaningful, but only Gate A is directly modeled and faults are mostly exclusive. Existing scope retained. |
| `release_reviews` | Added colliding IDs, nine independently altered identity/content fields, inclusive zero-width time windows, overlapping review faults, and rotation/reversal invariance. Raw generator fields determine the oracle, without production freshness/equality helpers. |
| `quality_acceptance` | Old family could never accept because review evidence was always missing. Added a complete positive fixture, mutations of every revision coordinate, review category/cycle faults, and severity/disposition interactions. |
| `gate_report` | Added near-valid one-defect cases, absent exit codes, exact-command drift, duplicate observations and record/observation overlap. Assert retention of every record, constraint and observation, not just verdict. |
| `cross_system` | Existing sixteen-state truth table meaningfully exercises obligation-to-settlement wiring. Generated booleans repeat the finite table; provider execution and daemon recovery remain outside its boundary. |

Stage 8 now uses source-adjacent failure persistence explicitly. Its integration-only workspace had no `lib.rs` or `main.rs`, so the old default emitted warnings and could not save generated seeds. The nested lockfile is reconciled with workspace v0.1.18; third-party versions and checksums are unchanged.

Run from the repository root:

```sh
RUSTC_WRAPPER= CARGO_PROFILE_DEV_DEBUG=0 CARGO_TARGET_DIR=target/stage8-policy \
  PROPTEST_RNG_SEED=9127 cargo test --locked --offline \
  --manifest-path discovery/proptest-praxis/stage8-policy/Cargo.toml
python3 discovery/proptest-praxis/campaign.py build --output /tmp/praxis-strengthened-build
python3 discovery/proptest-praxis/campaign.py search --target host --cases 1024 --seed 9127 --output /tmp/praxis-host
python3 discovery/proptest-praxis/campaign.py search --target scoped_read --cases 512 --seed 9127 --output /tmp/praxis-scoped-read
python3 discovery/proptest-praxis/campaign.py search --target sandbox --cases 512 --seed 9127 --output /tmp/praxis-sandbox
python3 discovery/proptest-praxis/campaign.py search --target process_output --cases 1024 --seed 9127 --output /tmp/praxis-process-output
```

Observed runs used seeds `9127` and `32189`. All 6,144 cases across the four strengthened public targets passed. Behavioral counters recorded 203 mid-scan cancellations, 104 same-size in-place mutation rejections, 99 pathname replacement rejections, 114 socket identity separation checks, and 3,027 IP prefix boundary probes. Every lease binding dimension was reached while live at least 435 times. These counts are exercised interactions, not new independent bugs.

The five Stage 8 suites passed 9,216 generated cases and 203 fixed scenarios per seed. Three compiled calibration defects were detected by the new fixed scenarios and then restored: ignored review-binding sequence, acceptance that always rejects, and a constant hash replacing the socket endpoint identity. The socket defect passed construction and failed specifically at the new identity-separation assertion. No production defect was confirmed by these bounded policy families.
