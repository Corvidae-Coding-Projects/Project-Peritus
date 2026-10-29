# I0 completion: execution-efficiency cost baseline

Status: complete for the initial cost-ledger increment. This does not qualify I1–I6 or claim
that the entire execution-efficiency design is complete.

## Implemented contract

- Each admitted attempt records its host role, turn, retry attempt, reason, normalized terminal
  outcome, observed/unknown usage and start/first-event/first-public-text/total elapsed time.
  Preparation, rendered projections, admission and confirmed vendor cache usage are distinct.
- Codex and OpenCode renderers report actual part bytes and Unicode scalar counts. Disjoint
  encoded spans are attributed to policy, schemas, exact-source blocks, conversation/mixed
  bodies, derived state, live control and framing/encoding. All seven classes occur in this
  real campaign; byte/character partitions sum exactly to their rendered parts. Attribution
  uses roles, block types and fixed host markers, not a claim to understand mixed text.
- Measurements reuse encoded buffers and borrowed JSON spans. They do not serialize the whole
  request again or retain prompt bodies. Prefix hashes remain local; common-prefix bytes are
  a lower bound and never a provider cache-hit claim. Vendor-owned framing remains outside
  these named adapter parts; absent adapter measurements are explicitly unavailable.
- Native OpenCode correction projections have a separate reason/outcome and preserve observed
  native usage before logical-request combination, including a later failure. Native counters
  must not be added to already combined logical counters. Unobserved native usage stays unknown.
- Context records include assembly/cache/index lookup work, environment refresh work, checkpoint
  commits and artifact store/read/write/verification/finalization/reuse counters. Store counts
  describe their documented boundary, not operating-system physical disk traffic. Managed
  fingerprint work also has the separately retained real Git capture benchmark described in
  the implementation ledger. No full-disk-I/O total or unseen synchronization count is invented.
- Host tool execution time includes top-level child work/waiting and must not be added again
  to child timings. First useful action is the first changed file or completed declared external
  operation; chat-only cases have no such action. Peak process-tree RSS is sampled, not exact.
- Diagnostics are content-free and non-authoritative. Disabling them removes their sidecars
  and optional rendering/state work while preserving canonical views, tools and accounting.

## Current-build real-operation qualification

The retained `cost-ledger-replay.py` drives actual ProductRunner, current Codex inference,
host file tools, local memory, real cancellation and native failure. Its `audit-usage` companion
uses the production canonical C5 decoder on durable provider envelopes, independently of the
new efficiency JSONL. No mocked provider or synthesized success is used.

All seven cases passed. The two paired chat/review tasks use identical source bytes, task,
workspace path, model and effort per pair, with fresh private run state. Configuration order is
reversed for review. Both configurations have byte-identical initial canonical provider views,
exact independent answers and unchanged files. Diagnostics-off creates no efficiency sidecar.
Reported cache acceptance is measured separately; no cold vendor-cache assumption is made.

| Case | Canonical responses | Input tokens | Cached input | Output tokens | Elapsed seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| chat / diagnostics off | 3 | 47072 | 15872 | 359 | 23.679 |
| chat / diagnostics on | 3 | 47117 | 7936 | 438 | 22.011 |
| review / diagnostics on | 2 | 31382 | 0 | 205 | 12.523 |
| review / diagnostics off | 2 | 31378 | 0 | 247 | 15.078 |
| unavailable-model | 1 | unknown | unknown | unknown | 2.444 |
| cancelled | 1 | unknown | unknown | unknown | 4.061 |
| managed | 7 | 146178 | 0 | 1205 | 68.745 |

Seventeen successful responses report 303,127 input, 23,808 cached input and 2,454 output tokens
in total. The failed and cancelled responses have unknown usage, not zero cost. Fourteen
admitted attempts have diagnostics; the other five successful responses intentionally run with
diagnostics disabled. Independent per-response high-water sums match cumulative accounting.
Optional cache-creation, reasoning, tool, total-token and cost fields remain unknown when absent.
Retries are recorded as zero where none occurred; no malformed provider response is fabricated.

The managed artifact case reaches `Accepted` through real writing, deterministic CSV validation
and independent review. It preserves exact requested bytes, the project manifest and original
Git HEAD/index. The first actual workspace write is observed at 22.530 seconds; acceptance is
observed at 68.745 seconds. Its seven provider attempts consume 67.706 seconds, and four measured
top-level tools consume 0.034 seconds. These are boundary-specific observations, not a universal
speedup claim. Its sampled process-tree RSS peak is 220,084 KiB. Sampling every 50 ms can miss
short-lived children. A formatting check overlapped initial campaign startup; latency rows are
observations, not a controlled estimate of instrumentation overhead.

## Completion evidence

- `cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui` passes.
- Affected all-target/all-feature strict Clippy passes, including both provider adapters, the
  context/agent owners, workspace runtime, product runner and daemon.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Real output, source-preservation, canonical-view equality, telemetry partition/privacy,
  failure/cancellation and independent usage-reconciliation oracles pass.
- All seven attributed component classes have nonzero actual rendered contributions. No
  fixture nonce, source body or absolute workspace path occurs in the efficiency logs.

Evidence root: `/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`.
The final campaign is `i0-completion-ledger/result.json`; raw cases, private traces and exact
inputs remain alongside it. Build, Clippy and formatting records are `i0-final-build-r2.log`,
`i0-final-clippy-r4.log` and `i0-completion-fmt.log`. `i0-completion-source-identity.json` and
`i0-completion-binaries.json` record exact identities. The ordinary consumer SHA-256 is
`9ec51be1e413bf7ff66cff2699f7dc4d90141c0aebf4745bc1dfde4f8e547c6c`.
The source inventory is a hash record, not a claim that every file was audited.

## Coverage and independent rollback

This initial ledger is qualified on Linux with actual Codex chat, explicit review, a managed
accepted edit, model-unavailable failure and in-flight cancellation. Live OpenCode correction/
layout qualification, other providers/platforms, module/interactive journeys and broad
performance campaigns remain explicitly outside this evidence and belong to the affected
later increments. Their unavailable measurements are not converted to passing samples.

Set `PERITUS_EFFICIENCY_LOG=0` or `off` to disable diagnostics. This does not select a different
provider, alter permissions or prompt contents, change checkpoint boundaries or erase history.
No optimization was enabled by this qualification, and no repository commit/push was made.
