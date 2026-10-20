# Runtime performance review after structural repairs

Reviewed after implementation commit `2a293f367` was pushed to develop. Measurements use the
actual Linux developer-profile programs (optimization level 1, no debug symbols), disposable
SQLite/daemon state, and native public-interface consumers. No unit tests or verification suites
were used. No live user run, installation, or provider configuration was changed.

## Findings and disposition

1. **Fixed during this review: metadata snapshots scaled with all historical runs.**
   The first publication implementation cloned the map of shared record pointers on every read.
   A native consumer measured 71.3 microseconds at 1,024 records and 1,176.5 microseconds at
   16,384. Published map views now share an immutable `Arc` root. Snapshot creation is constant
   time; the selected key lookup remains logarithmic. Writers copy metadata only when necessary
   to preserve an existing view. On the same consumer, read plus lookup measured 0.065 and
   0.081 microseconds respectively. These isolated API timings are not an end-to-end speedup
   claim. The consumer is compiled with the same rustc settings before/after; production IPC
   timings appear below. The 256-update concurrency, stable-view, same-key serialization,
   independent-key durable publication, lazy-load, and failed-load retry cases still pass.

2. **Follow-up working-set concern: selected historical payloads remain resident.**
   [publication.rs](../../crates/app/peritus-product-state/src/publication.rs) retains each
   successfully loaded record in its entry. This removes eager startup hydration, but browsing
   many different large historical runs can still grow resident state. This is an identifiable
   ownership lifetime, not evidence of leaked unreachable allocations. A future improvement is
   evictable inactive read-only payloads backed by the same durable projections; active run state,
   evidence, and recovery authority must remain intact. Cold hydration also executes synchronously
   through the observation request path in
   [request.rs](../../crates/app/peritus-daemon/src/session/request.rs). Moving that I/O into an
   owned bounded blocking job is a useful follow-up for responsiveness while opening large history.
   Cross-session timeout failure from this path was not observed in this campaign.

3. **Follow-up copying concern: metadata queries clone the complete conversation.**
   [workbench.rs](../../crates/app/peritus-daemon/src/product_run/workbench.rs) uses `store.load`
   before producing the small workbench snapshot. That clones retained inputs and other state;
   `ControlStore::inspect` already permits a borrowed projection. Cached IPC requests were fast
   in the measured sample, so this is a source-grounded optimization target for larger histories,
   not a demonstrated current latency blocker. Independent state/receipt checks must remain when
   changing this path.

## Actual daemon queries and memory

Six conversations contain 72 durably queued inputs from the behavioral campaign. A fresh daemon
received 8,000 alternating metadata queries over a negotiated client, in eight groups separated
by two-second waits. All returned valid workbench snapshots. This is a bounded 16.15-second
observation, not a long-duration leak test.

- Exactly six cold replays; 7,994 cache hits at the end.
- Estimated retained replay-cache bytes remained 1,196,546.
- Per-group median latency: 12.14–14.36 microseconds; p95: 13.60–17.12 microseconds.
- RSS samples: 30,340, 30,604, 30,604, 30,604, 30,604, 30,668, 30,668, 30,668 KiB.
- No continuing linear RSS increase appeared after warming. This sample cannot establish the
  absence of a multi-hour leak or the memory behavior of unrelated agent/tool workloads.
- Logical process reads increased about 1.29 MB across the sample. Read volume includes startup
  page faults, SQLite observation checks, client/session work, and diagnostics; it is not a
  direct count of disk device reads or source-code reads.

The earlier equivalent original-build alternating workload measured median metadata queries of
0.231 ms, versus 0.015 ms on the initial fixed build, with p95 0.503 ms versus 0.028 ms. Durable
input latency changed from median 1.815 ms to 1.613 ms. Provider/network latency was not measured
or attributed to these local changes; no model inference was requested.

## Startup and deferred history

The copied history has an actual governed continuation plus a synthetic 15.5 MiB retained
finding-state field. A first legacy JSON read remains necessary to build the disposable index.

| Observation | Original | First index migration | Warm indexed restart |
| --- | ---: | ---: | ---: |
| Readiness, seconds | 0.182 | 0.125 | 0.030 |
| RSS at readiness, MiB | 61.4 | 29.5 | 28.0 |
| Logical characters read at readiness | 18,110,770 | 16,921,824 | 593,778 |
| First selected-run query, seconds | 0.005 | 0.078 | 0.075 |

The first selected-run query pays for deferred validation/hydration. The improvement concerns
readiness and cold historical payload retention; the full payload is still loaded when needed.
The eight-case restart replay was repeated on the final shared-map implementation. Valid state,
missing/malformed/semantically invalid/stale indexes, corrupt/missing continuations, rejection,
and same-process repair all passed. Selected summaries stayed exact.

## Resource and dependency observations

Native plugin success/crash/flood/protocol-error/timeout cases were already run through the changed
host. A 2 MiB stderr flood is continuously drained into a 16 KiB retained ring; the public error
keeps the final causal message and status within its existing bounded format. No child process
survived those cases. This is process transport evidence, not a Wasm or cross-platform execution
claim.

The standalone TUI executable shrank from 26,615,992 to 23,859,624 bytes in the initial matched
builds. The CLI and daemon gained small amounts of code for the new behavior. Binary size is not
used as a proxy for runtime memory, compile throughput, or correctness. Both human clients no
longer depend on the agent runner for shared text, candidate identity, or trace decoding.

## Evidence and scope

The [behavior report](structural-runtime-real-2026-10-20.md) records the 56 TUI scenarios,
28 CLI scenarios, exact ten-state candidate identity comparison, 791-frame native trace read,
concurrency, cache invalidation, corruption/recovery, and plugin results. The original workspace
candidate-creation implementation was preserved; its diff remains empty.

Sources for the additional native performance consumers are in
[registry-review.rs](reproducers/structural-runtime/registry-review.rs) and
[cache-read-review.rs](reproducers/structural-runtime/cache-read-review.rs). Exact binaries,
configuration, copied state, startup logs, driver, and JSON samples remain outside Git in
`/home/doll/.cache/peritus-structural-20261020/`: `registry-review.jsonl`,
`registry-review-final.jsonl`, `cache-read-review.jsonl`, `cache-read-review-process.json`,
`history-copy/history-results.json`, `publication-final.jsonl`, and `review-build.log`.
