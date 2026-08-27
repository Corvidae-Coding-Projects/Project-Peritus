# peritus-benchmarks

`peritus-benchmarks` is the H3 production performance qualification core. It validates stable
profiles and workloads, lazily generates deterministic load and soak schedules, ingests bounded
measurements, accounts exact resource lifecycles and queue backpressure, compares an accepted
baseline, and produces content-bound evidence and report manifests.

## Invariants

- The crate owns no product, promotion, acceptance, or release authority.
- A subject adapter supplies a component-owned `Authorization` type. The harness only borrows it.
- Every workload has finite duration, rate, operation count, reservations, concurrency, queues, and
  a stable seed. Eight-hour schedules remain constant-memory iterators.
- Measurements bind exactly to one run, profile, and declared workload; sequence is contiguous,
  elapsed time is monotonic, and ingestion is bounded.
- Resource accounting rejects duplicate ownership, unmatched release, queue underflow/overflow,
  capacity overflow, and backpressure claims made before a queue is full.
- Percentiles use deterministic nearest-rank selection over integer metric units.
- `ready` requires every required workload definition, an exact complete runner receipt, workload
  measurements, sufficient SLO samples, balanced lifecycle resources, all SLOs met, and every
  policy-required baseline comparison. Blocking regressions prevent readiness.
- Evidence manifests sort artifact paths and bind subject bytes, runner bytes, profile bytes,
  workload bytes, machine description, time range, record count, and every retained artifact.

## Integration boundary

G0 and F0 implement `QualificationSubject` in an adapter crate or application test target. The
associated `Authorization` is their existing capability/authorization type. An external runner
implements `QualificationRunner`, owns timing and concurrency, and emits `MeasurementRecord` and
`ResourceEvent` observations through the supplied bounded sinks. H3 never calls an authority
transition and its `QualificationVerdict` is inert evidence consumed by the later release gate.

The checked-in dataset lives under the repository `benchmarks/` directory. Generated Criterion
statistics, load logs, soak logs, raw measurements, and reports belong in an external evidence
directory and are not source assets.

## Benchmark targets

The `qualification_core` Criterion 0.8.2 target uses `criterion_group!` with a Cargo
`harness = false` entry point that skips ordinary all-target test-runner invocations. It measures
lazy plan generation, validated measurement ingestion, and integer summary/SLO evaluation. These
microbenchmarks qualify harness overhead only; application SLO evidence must come from a G0/F0
subject adapter using the stable workload catalog.

See `docs/h3-performance-qualification.md` for the runner, evidence, baseline, and verdict protocol.
