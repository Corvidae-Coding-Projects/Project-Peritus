# Feature: C7 trace and telemetry

## Summary

C7 adds `peritus-trace` and `peritus-telemetry` as the observation boundary for Peritus. Trace
observations use canonical trace/span identities, exact causal predecessors, bounded closed
diagnostics, and explicit redaction results. Accepted observations are stored through C0's
transactional journal as inert family-60 frames under a dedicated `Trace` aggregate. The trace
projection is a pure C0 projection that can be rebuilt from genesis and installed through the
existing shadow-generation store. Telemetry derives metrics and OpenTelemetry-compatible export
values from that checked projection without receiving an execution command, authority receipt,
budget transition, or effect capability.

## User-visible behavior

1. A committed trace observation survives restart and either replays exactly or produces a typed
   integrity failure; it is never silently reordered, reparented, or duplicated with changed bytes.
2. Traces bind session, run, attempt, turn, action, provider profile, tool descriptor, gate, and
   gate-execution identities without using observability data to authorize any of those entities.
3. Default diagnostics contain only closed codes and typed scalar identifiers, counts, durations,
   statuses, and digests. Prompt text, model output, tool arguments, secret or credential bytes,
   environment values, and workspace content cannot enter the default event or metric value types.
4. Sensitive values are omitted or replaced by a digest-verified, finalized, encrypted artifact
   vault reference. Their contents are redacted from `Debug`, errors, metrics, and exporter values.
5. Export queues have a nonzero fixed capacity, deterministic reject-newest or drop-oldest policy,
   monotonic accepted/drop accounting, stable batch identities, retryable explicit failures, and a
   bounded shutdown flush.
6. Restart recovery compares a durable V2 final-disposition checkpoint with the rebuilt C0
   projection, rejects future or mismatched checkpoints, and reconstructs only the undisposed
   export range.

## Requirements

### R-C7-001 — canonical observation domain

`TraceId` is a nonzero 16-byte OpenTelemetry trace identity and `SpanId` is a nonzero 8-byte
OpenTelemetry span identity. An observation contains its C0 `EventId`, trace/span identities,
one-based span sequence, optional structural parent, canonical prior-event set, immutable causal
binding, caller-observed Unix nanoseconds and monotonic ticks, one closed observation kind, sorted
safe attributes, and sorted redaction decisions.

### R-C7-002 — causal binding and validation

A binding always names a session. Attempt requires run, turn requires attempt, action and
provider/tool bindings require turn, and gate execution requires both gate and attempt. A child span
may refine but never replace an ancestor's existing binding. Start, diagnostic, and end observations
obey exact span sequencing. Every structural-parent transition names the parent's latest event and
every additional causal event already exists in the same trace.

### R-C7-003 — duplicates and replay

The first observation identity binds its canonical frame digest. Reapplication with the same digest
is an explicit exact-duplicate outcome; reuse with changed bytes is an integrity failure. C0 replay
checks frame/aggregate trace identity, event identity, aggregate sequence, causal IDs, fixed schema
digest, and family/version before folding. Repeated genesis rebuilds produce byte-identical payload
and invariant digests. Each fold validates a temporary trace transition and publishes the trace,
event index, counters, and journal position only after every recoverable check succeeds.

### R-C7-004 — durable C0 storage

`JournalTraceStore` observes the current `Trace` aggregate head, validates the complete prior chain,
constructs an exact compare-and-append request, binds finalized vault artifacts as C0 artifact
dependencies, and resolves the command identity before retry. A successful result is an observation
receipt with positions and hashes only; it cannot authorize or dispatch work.

### R-C7-005 — redaction and vault references

The default attribute domain has no free-form text or opaque-byte variant. `SensitivePayload` has a
redacted `Debug` implementation and zeroizes its allocation on drop. Redaction consumes it. Without
artifact metadata the result is omission; with metadata, the bytes, size, SHA-256 digest,
finalization, quarantine state, and encryption metadata must all agree before an
`ArtifactVaultReference` is emitted.

### R-C7-006 — stable diagnostics and metrics

Closed diagnostic codes cover provider requests, tools, gates, budgets, retries, cancellation,
recovery, resource observations, exporter failures, drops, and shutdown. `peritus-telemetry` maps
these codes to stable monotonic counters with closed metric names and typed correlation dimensions.
Metrics never carry diagnostic text, raw input, or artifact contents.

### R-C7-007 — bounded buffering and exporter contract

The queue capacity and export batch size are validated and nonzero. Enqueue either accepts exactly
one item, rejects the new item, or evicts exactly one oldest item according to configured policy.
All outcomes update checked monotonic counters without clamping or wrapping. Export failure retains
the complete pending batch; success requires an acknowledgement matching its stable batch identity
and sequence range before removal.

### R-C7-008 — shutdown and restart recovery

Shutdown accepts an explicit maximum batch count, flushes within that bound, and returns
`Complete`, `Pending`, or a typed exporter failure. The export checkpoint is written using a bounded
canonical V2 payload, exclusive temporary file, file synchronization, atomic replacement, and
parent directory synchronization. A checkpoint covers the highest contiguous prefix in which every
record is finally exported or dropped; checked accounting requires
`exported + dropped == submitted == disposed_through`, while `accepted` independently records
historical admission. Recovery rejects corrupt, V1, future, wrong-stream, or
projection-mismatched checkpoints and deterministically accounts for observations that exceed the
restored buffer. Retrying an identical published generation repeats directory synchronization and
retention pruning before reporting success.

### R-C7-009 — OpenTelemetry-compatible boundary

Export spans use 16-byte trace and 8-byte span identities, optional parent span, Unix-nanosecond
start/end times, stable status, and closed typed attributes. Export events and metric points use the
same safe value domain. The exporter trait accepts immutable idempotent batches and returns a typed
matching acknowledgement or explicit redaction-safe failure.

### R-C7-010 — non-authority and formal verification

Public C7 APIs do not accept or return authority grants, policy changes, budget receipts, execution
commands, or effect permits. Executable Verus obligations cover exact sequencing, causal facts,
redaction decisions, replay-equivalence facts, authority preservation, bounded queue accounting,
monotonic accepted/drop/export sequences, and acknowledgement legality.

## Acceptance criteria

1. Both crates have small documented modules, typed stable errors, strict workspace lints, formal
   verification metadata, and no placeholders, ignored tests, hidden effects, or unsafe code.
2. Domain tests cover zero identities, invalid bindings, parent refinement, noncanonical/duplicate
   causal inputs and attributes, every span lifecycle failure, cross-trace parents, changed
   duplicates, monotonic time, and valid multi-root/multi-child traces.
3. Codec tests prove deterministic round trips, every closed variant, malformed tags, truncation,
   trailing bytes, collection bounds, and exact agreement with the journal envelope.
4. Storage tests use the real SQLite journal and artifact catalog to cover commit, exact command
   replay, conflict, stale head, encrypted vault dependency, restart, integrity export, corrupt
   frames, and deterministic projection rebuild/shadow installation.
5. Adversarial leakage tests place canaries in every sensitive category and assert absence from
   `Debug`, `Display`, error chains, encoded frames, projection payloads, metrics, and export values.
6. Buffer tests cover capacity one, exact capacity, one over under both policies, overflow refusal,
   exporter failure with retention, acknowledgement mismatch, bounded shutdown, and deterministic
   recovery/drop counts under load.
7. Export tests cover OTel identity widths, parent/status/timestamp projection, stable batch digest,
   retry idempotency, partial acknowledgement rejection, and explicit exporter shutdown failure.
8. Focused tests, formatting, strict Clippy, rustdoc with warnings denied, focused no-cheating Verus,
   architecture/source-layout checks, and the integrated workspace gates pass on all supported
   platforms.

## Current architecture

C0 provides an exact-byte SQLite journal, command idempotency, aggregate head CAS, integrity export,
artifact dependencies, pure projection contracts, and durable shadow-generation storage. B3 owns a
closed canonical framing registry. A1 supplies lifecycle/provider/gate identities but no trace or
span identities. C5 and D0 already keep provider and orchestration errors redaction-safe, but there
is no cross-subsystem trace, metric projection, export queue, or OpenTelemetry boundary.

The paired D1/C7 delivery reserves B3 families 50–52 for D1 and 60 for C7, C0 aggregate tag 7 for
gates, and tag 8 for traces. C7 owns the family-60 codec so the foundation layer does not depend on
the observe layer.

## Proposed design

### Crate boundaries

```text
peritus-trace
  -> peritus-types, peritus-codec, peritus-journal, peritus-projection,
     peritus-artifact-store, sha2, zeroize, vstd

peritus-telemetry
  -> peritus-trace, peritus-types, peritus-codec, sha2, vstd
```

`peritus-trace` owns domain, binding, codec, redaction, causal projection, C0 append storage,
recovery, and formal rules. `peritus-telemetry` owns derived metric state, OTel-safe projection,
bounded buffering, exporter contracts, durable export checkpoints, restart/shutdown recovery, and
formal accounting rules.

### Data flow

```text
authoritative B0–D1 fact
  -> typed safe observation + separately consumed sensitive payload
  -> redaction/optional encrypted vault reference
  -> family-60 canonical frame
  -> C0 Trace aggregate append
  -> C0 integrity export
  -> pure trace projection / shadow generation
  -> safe OTel + metric projection
  -> bounded idempotent export batch
  -> explicit acknowledgement / durable V2 final-disposition checkpoint
```

No arrow points back into an authority or execution interface.

### Preferred design and alternative

The preferred design uses a dedicated C0 trace aggregate and C0 projection generation. It shares
durability, integrity scanning, artifact references, replay, and repair behavior with other
authoritative facts while keeping observations inert.

Using the D0 `Agent` aggregate was considered but rejected because it would alias two independent
aggregate contracts, collide with agent identity derivation, and weaken corruption detection. A
standalone C7 SQLite store was also considered but rejected because it would duplicate C0
idempotency/recovery machinery and could drift from the authoritative journal position it observes.

## Data and compatibility

Family tag 60/schema 1 and aggregate tag 8 are immutable once released. The independently
domain-separated export-checkpoint format is V2; V1 is unsupported and fails closed. All fields use
big-endian fixed widths, explicit option tags, bounded collection counts, and closed discriminants.
Projection and checkpoint encodings are domain separated. Unsupported family versions, changed
schema digests, and unknown enum tags fail closed. C0 schema evolution must migrate the aggregate
kind check constraint without rewriting event bytes.

## Failure handling

Domain, codec, journal, projection, artifact, I/O, export, checkpoint, capacity, sequence, and
integrity failures are typed. Default formatting reports a stable code, operation, and static safe
detail; sensitive values and underlying effect-library messages are not included in `Debug`.
Indeterminate C0 commits are resolved with the same command identity and digest. Export failures
retain pending values and checkpoint writes use atomic replacement. Recovery never assumes an
unacknowledged batch succeeded.

## Security considerations

Free-form diagnostic strings and arbitrary bytes do not exist in the default observation, metric,
or export schemas. IDs and digests are treated as correlation data, never capability data.
Sensitive bytes are zeroized and only their omission or an encrypted artifact reference can cross
the redaction boundary. Vault references require exact digest/size agreement with finalized active
metadata. Metric labels are closed enums. Manual `Debug` and error implementations exclude raw
sources and buffers. Telemetry cannot construct or mutate execution authority.

## Verification

Focused verification consists of each crate's unit/integration/adversarial tests, `cargo fmt`,
strict all-feature Clippy, rustdoc with warnings denied, and the crate's Verus target. Integration
then runs generated schema consistency, architecture/source-layout/ordinary-API checks,
dependency/license policy, and the serialized workspace Gate A. The root agent coordinates all
Cargo and Verus execution.

## Rollout and rollback

The rollout registers both crates, C0 aggregate tag 8, B3 family 60, generated registries,
architecture ownership, and verification commands in one release. Before any family-60 record is
committed, rollback is removal of registrations and crates. After data exists, binaries may stop
exporting but must retain tag 8/family 60 decoding and C0 migration support; stored events are
immutable and are not rewritten.

## Open questions

None. Family 60 and aggregate tag 8 were accepted by the integration owner. Export destination,
credentials, scheduling, and retention policy are deployment concerns supplied through the
exporter/checkpoint interfaces.

## Out of scope

- Provider SDK or collector-specific network transports.
- An operator daemon, global scheduler, sampling policy, or credential manager.
- Mutation of B0/B1 authority, D0 execution, D1 gate truth, or acceptance state.
- Reading raw artifacts back from the vault through a telemetry API.
- Distributed clock synchronization or treating trace timestamps as authoritative ordering.
