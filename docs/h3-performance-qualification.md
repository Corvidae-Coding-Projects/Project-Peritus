# H3 performance qualification

H3 qualifies the integrated G0 daemon and F0 evolution surface against stable, application-relevant
load and soak workloads. It publishes measurements and a derived verdict; it cannot approve a run,
promote an evolution candidate, weaken correctness, or release an artifact.

## What is qualified

The version-1 metric vocabulary covers:

- daemon startup, command-to-first-event, authoritative event append, cancellation, recovery,
  garbage-collection pause, queue saturation, provider backpressure, and exporter backpressure
  latency;
- terminal, projection rebuild, process, token, and disk throughput;
- steady memory per active run and streamed process, peak resident memory, disk use, and token use;
- supported run, process, and provider concurrency plus command, terminal, exporter, and provider
  queue depth; and
- cancellation and recovery success ratios.

Every value is an integer in the unit fixed by `Metric`. Percentiles are nearest-rank p50, p95, and
p99, eliminating floating-point and interpolation drift across runners.

## Stable inputs

`benchmarks/workloads/production-v1.json` declares eleven focused load workloads and four eight-hour
soaks. A workload fixes its logical duration, operation rate and count, maximum concurrency, payload
size, memory/disk/token reservation, queue capacity, and seed. `QualificationPlan` generates any
step directly from its sequence, so an eight-hour schedule does not allocate a multi-million-entry
vector.

`benchmarks/profiles/qualification-candidate-v1.json` declares the candidate reference-machine
class, resource envelope, objectives, minimum samples, and regression thresholds. It deliberately
sets `baseline_required` to true. The target values are candidate acceptance limits, not a claim
that the current product meets them. No source baseline is established by H3 implementation work.

The JSON contracts in `benchmarks/schemas/` are stable interchange schemas. The Rust loader also
rejects unknown fields, unsupported schema versions, invalid identifiers, duplicate keys, dangling
workload references, invalid objective direction, oversized documents, and workload reservations
that exceed the selected profile.

## G0 and F0 adapter contract

An integration supplies two implementations:

1. `QualificationSubject` maps each `PlannedOperation` onto the disposable G0/F0 subject. Its
   associated `Authorization` is the integrating component's existing authorization/capability
   type. H3 receives only `&Authorization`; it has no way to mint, widen, persist, or approve it.
2. `QualificationRunner<Subject>` owns monotonic timing, schedule pacing, bounded concurrency,
   cancellation, disposable subject lifecycle, and terminal completion. It returns a
   `RunnerReceipt` for each workload.

The adapter records timing and gauge observations through `MeasurementSink`. It records exact
resource ownership and queue changes through `AccountingSink`. An observation does not assert that
the corresponding operation was authorized, durable, accepted, promoted, or released.

The G0 adapter should map, at minimum:

- run start/finish, event append, terminal chunks, process start/finish, cancellation, and restart;
- command, terminal, provider, and exporter queue push/pop and producer-visible full-queue wait;
- provider request start/finish and token accounting; and
- incremental artifact writes, quota observation, collection, and resident-memory sampling.

The F0 adapter uses the same measurement vocabulary for representative evaluation/evolution
campaigns. H3 must not receive the production pointer transition or promotion approval as a harness
operation.

## Bounded accounting and backpressure

`ResourceAccountant` is initialized from the profile `ResourceEnvelope`. Each lifecycle event is
applied atomically. A rejected event leaves the ledger unchanged. It tracks current and peak runs,
processes, provider requests, resident bytes, retained disk, tokens, every queue depth, accumulated
backpressure wait, and saturation observations.

A producer may report `BackpressureObserved` only while the selected queue is exactly full. This
distinguishes real bounded backpressure from a latency label attached after the fact. Workload
qualification additionally checks that declared run/process/provider concurrency or saturation
queue capacity was actually reached. A complete runner with unexercised bounds is `not_ready`.

Runs, processes, provider requests, queues, and resident memory must be balanced at terminal
evaluation. Durable artifact bytes and spent tokens may remain accounted within their profile caps;
they are retained evidence/consumption rather than leaked ownership.

## Measurement ingestion

Each `MeasurementRecord` contains exact run, profile, workload, metric, contiguous sequence,
monotonic elapsed microseconds, and integer value. `MeasurementIngestor` rejects mismatched bindings,
unknown workloads, sequence gaps, backwards time, invalid basis-point values, and records beyond the
profile limit. It can ingest typed records or bounded newline-delimited JSON.

Runners should use one monotonic origin for a qualification run. Wall-clock timestamps belong only
in the evidence manifest. Raw provider content, terminal content, secrets, or repository content do
not belong in metric records; retain required redacted raw evidence as separately digested artifacts.

## Evaluation and regression

`QualificationEvaluator` performs these checks in deterministic order:

1. Validate profile/run bindings, unique workload receipts, and workload reservations.
2. Compute integer minimum, mean, p50, p95, p99, maximum, and checked total by workload and metric.
3. Require every profile workload definition, an exact receipt whose expected count equals the
   stable workload operation count, completed execution, and at least one measurement.
4. Require the declared concurrency and saturation levels to have been exercised and lifecycle
   accounting to be balanced.
5. Evaluate every workload-scoped SLO at its minimum sample count.
6. Compare the same statistic to an exact profile/workload/metric/statistic baseline. Improvement
   direction comes from the metric; absolute materiality is applied before relative warning and
   blocking thresholds.

Warnings remain visible but do not by themselves block a candidate that still meets its SLO.
Blocking regressions, missing required baseline entries, missed SLOs, insufficient samples,
incomplete runners, missing coverage, or resource failures produce `not_ready` reasons. An absent or
failed observation is never converted into a favorable zero.

## Evidence protocol

Retain the exact bytes for:

- profile and workload catalog;
- subject executable and runner implementation identity;
- newline-delimited measurements;
- runner receipts and resource-accounting export;
- platform/machine probe output needed to establish the reference-machine match; and
- the structured qualification report.

`EvidenceManifest` binds subject and runner digests, reference machine, dataset digests, wall-clock
range, measurement count, and a path-sorted list of artifact length/digest pairs. Paths are relative
and cannot escape the evidence root. `QualificationReport` binds the exact manifest digest and the
complete evaluation. Compact JSON is deterministic and used for digests; pretty JSON is a display
form of the same report.

An accepted baseline document must conform to `baseline-v1.schema.json` and bind the digest of its
reviewed source evidence manifest. Criterion point estimates are not application baselines.
Generated output must remain outside the checked-in `benchmarks/` tree.

## Readiness handoff

`QualificationVerdict::Ready` means only that the supplied H3 inputs satisfy the H3 evaluator. The
H4 release owner must independently verify evidence retention, reference-machine identity, artifact
digests, platform/security/resilience gates, and the release-candidate binding before using H3 as one
input to a final release decision. Neither an H3 report nor an accepted baseline performs that
decision.
