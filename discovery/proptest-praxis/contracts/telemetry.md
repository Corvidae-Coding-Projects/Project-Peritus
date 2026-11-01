# Telemetry pilot contract and coverage record

Baseline: `d106c9b13318fbcdd3d816ed63ae48532806fa7e`. This is discovery machinery only. No production edits are part of this pilot.

## Contract selected

`TelemetryBuffer` provides bounded FIFO retention under RejectNewest and DropOldest. Each submission receives a stable one-based sequence, including rejected records. Accepted counts historical admissions; dropped includes rejection and eviction; exported changes only after exact whole-batch acknowledgement. Queue length never exceeds its configured capacity.

`TelemetryPump::flush_one` exports at most the configured batch size. An empty queue never calls the exporter. Explicit exporter failure or a mismatched stream, batch identity, first/last sequence, or count retains the queue and its counters. Repeating the unchanged batch preserves identity and encoded bytes. The contract is at-least-once export: an exporter may receive the same batch repeatedly before a valid acknowledgement.

The disposed boundary is the longest prefix of submissions whose dispositions are all known (dropped or exported). Disposed counters describe exactly that prefix. Rejected sequences behind a pending accepted sequence cannot be checkpointed until that earlier sequence is resolved.

`shutdown(maximum_batches)` is bounded: it returns Pending while records remain, calls exporter shutdown only when empty, and propagates flush/ack/shutdown errors. No permanent closed state is promised. This pilot therefore continues using the pump after shutdown and after failures.

## Independent oracle

An append-only `Vec<Submission>` stores a small synthetic value, whether it was ever admitted, and Pending/Exported/Dropped. It recomputes pending records by a full scan and the disposed boundary by scanning from genesis. Counters are counted from this history, not updated using production formulas. No production `verified.rs`, queue, gap metadata, prefix helper, or result from the subject selects generated operands.

Each enqueue/flush/shutdown result, global and disposed counter, disposed sequence, length, and emptiness is compared after each command. Exporter observations verify exact record order/value/sequence, metric name, both time fields, trace identity, stream identity, and first/last sequence. Expected metric metadata is stated directly as independent fixture constants. An exporter-side map checks stable batch ID/bytes for repeated unchanged logical contents. At replay end a separate buffer clone is drained through the public pump to inspect the entire retained queue; the original replay remains unchanged. The disposed-prefix digest itself is not independently recomputed by this pilot.

Records use public `MetricPoint::new`, `ObservedTime::new`, `TraceId::new`, and `ExportRecord::Metric` constructors. Small repeated metric values deliberately exercise duplicate payloads while submitted sequence remains distinct. No trace/journal projection is needed to construct this valid record variant.

## Read coverage

Fully read implementation and contract files:

- `crates/observe/peritus-telemetry/Cargo.toml`, `README.md`, `src/lib.rs`, `src/error.rs`.
- `src/buffer/mod.rs`, `src/buffer/state.rs`: configuration, enqueue, eviction/rejection, batching, acknowledgement, disposition and error paths.
- `src/export/types.rs`, `src/export/pump.rs`: public exporter, acknowledgement, whole-batch errors, bounded shutdown.
- `src/metrics.rs`, `src/projection.rs`: public synthetic metric construction and actual production record producers.
- `src/recovery.rs`, `src/storage.rs`: downstream meaning of disposed checkpoints; these are read for contract context, not exercised by this pilot.
- `crates/observe/peritus-trace/src/identity.rs` and `src/domain/observation.rs`: record fixture constructor constraints.
- `crates/app/peritus-daemon/src/telemetry/runtime.rs`: actual recovery, flush/checkpoint and shutdown caller, including typed daemon failure propagation.

The nearest root, observe, telemetry, source, buffer/export and trace navigation documents were read. Workspace Rust toolchain and lint guidance are retained; the standalone consumer does not modify production interfaces.

## Search support and metrics

Generator configurations cover capacities 1–8, every valid batch size, both policies, repeated payload values, three operation mixes, and bounded general histories. A constructive family fills beyond capacity, fails/retries export, corrupts an ack, flushes, appends beyond a rejection gap, exercises zero-bound shutdown, fails exporter shutdown, then recovers and resumes. Inputs contain all dependency responses; replay has no hidden randomness. Shrinking can remove operations without creating invalid handle references.

Deterministic examples cover each ack corruption independently, retryable and terminal exporter errors, full queues, noncontiguous submitted sequences, rejection gaps, partial bounded shutdown, exporter shutdown failure, and reuse after completion. A per-shutdown export ordinal permits several successful batches before a scripted export/ack failure. Two explicit cases retain acknowledged progress after a later batch fails, then retry the remaining queue. The optional `accepted_batches_before_response` field defaults to zero when reading existing materialized replay inputs.

Counters include enqueues, accepted-without-eviction, oldest evictions, newest rejections, empty/successful/failed flushes, exporter calls and each actual response class, repeated batch identity comparisons, noncontiguous batches, shutdown attempts/pending/completed/failures, shutdown failures after acknowledged progress, drops behind an unresolved prefix, maximum queue size, clone queue checks, operations, and both policies. A high random case count without these events does not establish coverage.

## Calibration

`calibration/telemetry.patch` deliberately removes the increment of the cumulative dropped count on a full enqueue. The exact witness is capacity 1 / batch size 1 / RejectNewest / enqueue value 1 / enqueue value 2. The expected failed assertion is the global counter comparison after step 1: expected `[2, 1, 1, 0]`; the mutant reports `[2, 1, 0, 0]`. It changes a real production counter without preventing compilation. Apply only in the disposable calibration checkout, run the explicit witness and generated search, then revert the deliberate mutation before recording a clean campaign result.

## Boundaries and outstanding execution

This pilot does not exercise persistent checkpoint I/O/restart reconstruction, span/event projection, redaction, exporter file publication, native shutdown, concurrency, integer-overflow reconstruction, or arbitrary invalid configuration. Those remain distinct campaign targets. Batch IDs and their encoded bytes are checked for stability and correlation, not independently cryptographically rederived; disposed-prefix digest contents are outside this pilot's oracle. Export adapter callbacks are synchronous and deterministic.

Stage 1 builds, fixed replays, two standard generated searches, deliberate dropped-counter mutation detection/shrinking, restored replay, strict lint and formatting passed. Actual observations and exact tested identities are in `../REPORT.md` and `../evidence/stage1.json`. A post-calibration Clippy fix factored the unchanged batch-identity map type into a type alias; it changed no generator, oracle or operation behavior. Final baseline searches use that version. Build orchestration remains serial.
