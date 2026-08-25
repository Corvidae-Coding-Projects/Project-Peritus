# C7 trace and telemetry operations

C7 records durable, causally checked observations and projects them into bounded telemetry. It is
an observation path only. A trace receipt, span, diagnostic, metric, export acknowledgement, or
checkpoint never grants permission, spends budget, changes policy, dispatches work, or establishes
that authoritative work succeeded.

## Contracts and ownership

`peritus-trace` owns the family-60/schema-1 canonical observation format and the C0 `Trace`
aggregate contract. `peritus-telemetry` consumes a checked trace projection and owns safe
OpenTelemetry-shaped values, stable metric counters, bounded buffering, exporter batches, shutdown,
and local export checkpoints.

The durable path is:

```text
authoritative subsystem fact
  -> closed observation fields + explicit sensitive-value redaction
  -> family-60 frame in a C0 Trace aggregate
  -> integrity-checked trace projection rebuilt from C0
  -> safe spans, events, and metric points
  -> bounded export queue and immutable idempotent batches
  -> exact acknowledgement and durable V2 final-disposition checkpoint
```

There is deliberately no path from the last four stages back to an authority or execution API.

## Recording observations

Create a nonzero 16-byte `TraceId` and a nonzero 8-byte `SpanId`. An observation also names its
durable C0 `EventId`, a one-based sequence within its span, the optional structural parent span,
the exact prior events on which it causally depends, a `CausalBinding`, caller-observed time, one
closed observation kind, sorted safe attributes, and sorted redaction decisions.

Projection application is failure-atomic: a rejected lifecycle, parent, session, causal, sequence,
time, or counter check publishes no trace, span, event index, counter, or journal-position change.
It is therefore safe to correct a rejected observation and retry against the unchanged state.

`CausalBinding` always includes the existing session. Its refinements follow the domain hierarchy:

- attempt requires run;
- turn requires attempt;
- action, provider profile, and tool descriptor require turn;
- gate execution requires both gate and attempt;
- a child span preserves every identity fixed by its parent binding.

The trace projection enforces the span lifecycle. A start is sequence one. Every later event names
the span's latest durable event and increments the sequence exactly once. A child start names the
parent's latest event, all named causal events already exist in the same trace, observed time cannot
regress within a span, and a closed span cannot advance. Event identity reuse with identical
canonical bytes is an exact duplicate; reuse with changed bytes is an integrity failure.

Use `JournalTraceStore::record` with a stable `CommandId`. The recorder resolves that command before
retrying, rebuilds the trace's checked prior state, and issues one C0 compare-and-append. On an
indeterminate persistence response, retry the same command identity and exact observation. Do not
mint a new event or command until resolution is known. A returned `RecordedObservation` contains
only C0 identity, position, and hashes; it is not an execution receipt.

## Redaction and artifact evidence

Default observation attributes are a closed enum of identifiers, digests, counts, durations,
statuses, and vault references. They cannot contain arbitrary text or bytes. Never transform prompt
text, model output, tool arguments, credentials, environment data, or workspace content into a
supposedly safe string label.

Pass sensitive bytes through `SensitivePayload` and `redact_sensitive`:

- With no artifact metadata, the bytes are consumed and the observation retains only sensitivity
  class and byte count.
- With artifact metadata, the bytes must match its exact SHA-256 digest and size. The artifact must
  be finalized, active rather than quarantined, and envelope encrypted. Only then is an
  `ArtifactVaultReference` returned.

The vault reference contains an artifact digest, encrypted object size, creating event, opaque key
reference digest, and encryption-parameter digest. C7 provides no API to read artifact contents.
When a vault reference is committed, its digest is included as a C0 artifact dependency, so the
journal transaction fails if the finalized catalog entry is absent.

`SensitivePayload`, trace errors, telemetry errors, exporter errors, metrics, and default export
values have content-free formatting. Underlying journal, codec, projection, filesystem, and adapter
errors are classified but are not exposed via `Error::source()`. Operator logs should record the
stable error code, operation, recovery class, and closed exporter class only.

## Rebuild and recovery

`recover_trace` rebuilds one trace aggregate from its complete checked C0 chain. `recover_all`
first obtains a C0 integrity export and then folds every family-60 record from genesis through the
pure `TraceProjection`. Unknown families are ignored by this projection; malformed trace frames,
schema drift, envelope disagreement, chain failures, and causal failures stop recovery.

The trace projection is replaceable. A projection database may be rebuilt into a shadow generation,
validated, and activated through the C0 projection-store workflow. The C0 journal remains the
source of truth. Never repair trace state by editing a projection payload or skipping a failed
record.

`project_telemetry` deterministically orders observations by C0 global position. Diagnostic
observations become safe events and, where defined, monotonic metric points. A terminal observation
emits one completed OpenTelemetry-compatible span with 16-byte trace identity, 8-byte span identity,
optional parent, observed start/end times, closed outcome, start attributes, and ordered diagnostic
events. Start observations alone are not exported as incomplete spans.

## Buffering and exporter behavior

Create `BufferConfig` with nonzero capacity and batch size. Batch size cannot exceed capacity and
capacity cannot exceed the hard one-million-item bound. Select one deterministic full-queue policy:

- `RejectNewest` retains the queue and rejects the arriving record.
- `DropOldest` evicts exactly one oldest record and accepts the arriving record.

Every submission receives a checked stable sequence and advances a domain-separated projection
prefix. `BufferCounters` separately report submitted, accepted, dropped, and exporter-acknowledged
records. `accepted` is historical admission accounting independent of whether an accepted record is
later exported or evicted. Alert on `dropped` growth; it is never silently clamped or reset.

The pump separately tracks its highest contiguous final-disposition prefix. A record is finally
disposed only when an exact exporter acknowledgement removes it or the configured policy drops it.
Rejected and evicted gaps advance the boundary only after all earlier sequences are also disposed.
At that boundary, checked accounting requires
`exported + dropped == submitted == disposed_through`; live queue counters may already extend beyond
the checkpointable prefix.

An `Exporter` receives an immutable `ExportBatch`. Its batch identity covers the stream identity,
record sequences, and canonical redaction-safe record bytes. On failure, the complete batch remains
pending and may be retried unchanged only when the returned stable exporter class says it is
retryable. A non-retryable failure has terminal exporter recovery guidance while still retaining the
batch for operator disposition. Success requires an `ExportAck` matching the stream, batch identity, first and last
sequence, and complete count. Partial or contradictory acknowledgement is rejected and removes
nothing.

Exporter implementations must discard provider response bodies, transport URLs, credentials, and
headers before returning `ExporterError`. Use only `ExporterErrorCode` and the retryable bit. A
permanently rejected batch requires operator policy outside C7; C7 neither drops it automatically
nor changes authoritative work to make it succeed.

## Checkpoint and restart sequence

`CheckpointStore` is a single-owner directory. Checkpoint format V2 stores the exact contiguous
final-disposition sequence, projection-prefix digest, and boundary counters. It writes a fixed-size,
checksummed checkpoint to an exclusive temporary file, synchronizes it, atomically renames it to a
stream-and-sequence generation, synchronizes the parent directory, and prunes only older published
generations beyond configured retention. Retrying identical bytes for an existing generation
repeats directory synchronization and retention pruning, covering a prior attempt whose rename
succeeded but directory synchronization failed. Startup removes abandoned temporary files for that
stream. V1 and unknown checkpoint markers, as well as a corrupt highest published generation, fail
closed; recovery does not fall back and hide the failure.

Use this shutdown sequence:

1. Stop producers from submitting new telemetry records.
2. Call `TelemetryPump::shutdown` with an explicit maximum number of batches.
3. If it returns `Pending`, preserve the C0 journal and current checkpoint, then continue bounded
   flushing or stop. Do not claim that pending records were exported.
4. After each contiguous final-disposition boundary that must survive restart, persist
   `ExportCheckpoint::from_pump`. This includes boundaries advanced by an explicitly accounted
   policy drop, not only exporter acknowledgements.
5. Treat exporter shutdown failure as explicit even when the queue is empty.

Use this startup sequence:

1. Open C0 and run its integrity checks.
2. Rebuild the checked trace projection from C0.
3. Derive the telemetry projection.
4. Open the checkpoint store and load the highest published generation.
5. Call `recover_buffer` with the same stream identity and current `BufferConfig`.
6. Resume export from the returned pump.

Recovery verifies stream identity, ensures the disposed-through sequence is not in the future, and
recomputes the exact projection prefix through that sequence. It then enqueues only the suffix under
the configured capacity policy. `RecoveryReport` exposes replay and recovery-drop counts. A prefix
mismatch indicates changed/corrupt history or the wrong checkpoint; do not reset it automatically.

## Stable diagnostics and metrics

Trace error codes use the `PERITUS-TRACE-*` namespace. Telemetry errors use
`PERITUS-TELEMETRY-*`. Branch on the typed kind and recovery class rather than parsing display text.
Telemetry exporter failures additionally expose a closed `ExporterErrorCode` and retryable bit.

The closed metric set counts provider requests/failures, tool dispatches/failures, gate
executions/passes/failures, budget events, retries, cancellation, recovery, resource observations,
exporter failures, buffer drops, and shutdown operations. Metric points contain only name,
cumulative value, observed time, and trace identity. Buffer counters are operational queue state,
not derived authority or billing records.

Recommended alerts are sustained buffer drops, a non-draining pending queue, repeated retryable
export failures, any non-retryable exporter failure, checkpoint storage failure, checkpoint-prefix
mismatch, trace integrity failure, or shutdown that repeatedly exhausts its configured bound.

## Verification and compatibility

Family 60/schema 1 and C0 aggregate tag 8 are permanent compatibility assignments. The local export
checkpoint is independently versioned at V2; V1 is unsupported. Canonical fields use fixed widths,
big-endian integers, explicit option tags, bounded collections, and closed discriminants. Unknown or
malformed values fail closed.

Focused verification covers domain and lifecycle adversaries, canonical round trips, redaction
canaries and complete error chains, encrypted vault requirements, real SQLite commit/replay/restart,
projection determinism, bounded load and drop accounting, stable batch retry, contradictory
acknowledgements, shutdown bounds, checkpoint corruption, abandoned temporaries, and prefix-checked
recovery. Formal obligations cover causal sequencing, replay equivalence, redaction decisions,
non-authority, bounded accounting, monotonic counters, and exact acknowledgements.
