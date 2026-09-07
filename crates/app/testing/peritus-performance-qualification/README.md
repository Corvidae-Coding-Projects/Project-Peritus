# peritus-performance-qualification

`peritus-performance-qualification` owns the executable H3 integration boundary. It applies the
stable plans from `peritus-benchmarks` to disposable G0/F0 subjects with monotonic wall-clock
pacing, cooperative cancellation, bounded accounting, and evidence-ready results.

The crate never uses paid provider accounts for load or soak traffic. Provider-pressure scenarios
use a deterministic local adapter. Production-profile results fail closed unless the measured host
matches the profile's declared reference-machine class; accelerated smoke runs are separate
non-release evidence and cannot produce an H3 `Ready` verdict.

The integrated subject launches a disposable `peritusd`, negotiates the public A3 protocol, and
submits real fenced scheduler commands through the same command boundary as an application client.
Terminal, cancellation, artifact, queue, and provider-pressure operations use owned local effects;
the provider adapter is deterministic and never reads provider credentials. Each subject capability
is created with its disposable instance and cannot authorize another instance.

Generated measurements and reports belong in an operator-selected directory outside the
repository. The campaign coordinator first runs short load workloads sequentially. In full mode it
then runs the four eight-hour workloads concurrently under one combined resource envelope, so the
production soak takes eight hours rather than thirty-two. Every subject must report the same exact
daemon executable identity.

The runner will not start unexecuted backlog after a workload's declared time window. It retains
a failed receipt with the actual executed count and elapsed/declaration values. An operation that
started inside the window may finish afterward; an early-finishing plan waits until its declared
horizon, with cancellation still active. A non-completed receipt stops later loads and prevents
the soak phase; a non-completed soak cancels its sibling workers. The report preserves partial
measurements and the failed receipt after observed subject cleanup.

Focused `event_append` workloads use `EventAppendRunner`: one independent monotonic producer,
the declared bounded worker concurrency (up to 32), and the declared bounded client queue.
Arrivals that miss their entire rate interval are recorded as missed, not burst-replayed. Full
queues reject arrivals explicitly. Queued work expires at the horizon; already-started work drains.
Every unsampled arrival, completion, rejection, expiry, and terminal counter is retained in
`results/events.<workload>.ndjson`, independently of the latency reservoir.

Every `AppendEvent` now submits a real synthetic D2 review finding through public A3. Its canonical
family-54 **event payload**, excluding the 16-byte frame header, has exactly the plan's seeded size.
Each operation uses a fresh review aggregate and requires two prerequisite commits (start and
assignment) before the measured submission. These are not extra successful workload operations;
the focused runner counts them separately and includes preparation, scheduled waiting, queueing,
all three public commands, and commit acknowledgement in event latency. Synthetic findings are
not real reviews, evidence approvals, or release authority. This fixture is not comparable to the
old scheduler-toggle diagnostic or an old adapter baseline.

This correction covers event payloads and the focused event-append arrival schedule. Other scenario
adapters still include local pressure effects; it is not full H3 qualification.

Long campaigns use deterministic reservoir sampling per workload and metric. Objective metrics
retain twice their required sample count and diagnostics retain a bounded representative set, then
the coordinator merges everything into one monotonic campaign sequence. Queue workloads end with
an exact drain when their operation count stops partway through a saturation cycle, so successful
plans return the shared ledger to a balanced terminal state.

Reservoir draws use the per-metric observation ordinal, independently of global measurement
sequence numbers. Matching counters must not cancel the sampling seed or bias retention toward
early events; interleaving another metric leaves the selected event ordinals unchanged.

`CampaignEvidenceWriter` publishes a completed campaign through a private temporary directory and
one final rename. It refuses an existing destination, reparses the exact profile, workload, and
optional baseline documents, and requires them to equal the typed inputs that were executed. It
streams copies of `peritusd` and the qualification runner while recomputing their recorded SHA-256
identities, then retains measurements, receipts, accounting, machine facts, the content-addressed
manifest, and its bound report. A failure never creates the requested final bundle path.

The test-only `tests/fixtures/general-capability/performance/` matrix checks a measured
improvement, missing comparison evidence, and a plausible change that is measurably slower. The
same qualification evaluator used by H3 must block the regression even when its absolute SLO still
passes.

## Operator commands

Build `peritusd` and the H3 operator, then run the first complete campaign without a baseline:

```sh
CARGO_BUILD_JOBS=2 cargo build --locked --bin peritusd --bin peritus-h3
target/debug/peritus-h3 full \
  --daemon target/debug/peritusd \
  --scratch /path/on/reviewed-storage \
  --profile benchmarks/profiles/qualification-intel-core-ultra-9-275hx-v1.json \
  --workloads benchmarks/workloads/production-v1.json \
  --evidence /path/to/new/peritus-h3-baseline-evidence \
  --storage-class nvme-gen4 \
  --revision "$(git rev-parse HEAD)"
```

That profile identifies the retained Intel qualification host. Use
`benchmarks/profiles/qualification-candidate-v1.json` only on its declared AMD reference machine;
the operator rejects either profile on a different host before starting a campaign.

`load` runs the sub-hour catalog. `full` runs that catalog and then the four concurrent eight-hour
workloads. The command probes the operating system, architecture, CPU, logical cores, and memory;
the storage generation remains explicit because unprivileged operating-system interfaces do not
report it consistently. Both raw CPU/memory facts and their normalized hardware class are retained.
The command fails before launching `peritusd` if that class does not exactly match the profile.

`--scratch` is required and must name an existing directory on the reviewed storage. There is no
ambient `/tmp` or `TMPDIR` fallback: a RAM-backed temporary filesystem cannot stand in for the
declared NVMe device. Choose a short canonical path to accommodate native Unix-socket limits.
The runner records each workload's actual canonical directory, filesystem device, and inode in
`results/storage.json`, and requires process reaping and directory removal to succeed before
retaining a successful cleanup observation. Review the mount and device mapping separately; the
numeric filesystem identity alone does not establish an NVMe generation. The owned parent remains.

The first run is expected to finish `NotReady` because no accepted baseline was supplied. A baseline
candidate requires complete runner coverage, resource exercise/accounting and sufficient objective
samples. Failed, cancelled or incomplete execution cannot produce `baseline-candidate.json`, even
if its partial latency samples meet an objective. Observed SLO misses or regressions remain visible
and do not alone prevent generating an inert candidate for explicit review. Review that
file and its bound manifest, then run a separate complete comparison with the reviewed candidate and
its exact file digest:

```sh
sha256sum /path/to/peritus-h3-baseline-evidence/baseline-candidate.json
target/debug/peritus-h3 full \
  --daemon target/debug/peritusd \
  --scratch /path/on/reviewed-storage \
  --profile benchmarks/profiles/qualification-intel-core-ultra-9-275hx-v1.json \
  --workloads benchmarks/workloads/production-v1.json \
  --baseline /path/to/peritus-h3-baseline-evidence/baseline-candidate.json \
  --accept-baseline-sha256 <reviewed-document-sha256> \
  --evidence /path/to/new/peritus-h3-comparison-evidence \
  --storage-class nvme-gen4 \
  --revision "$(git rev-parse HEAD)"
```

The command rejects either baseline option alone and rejects any byte change after review. This
explicit action admits the baseline for H3 comparison; it does not grant release authority. A
completed `NotReady` campaign publishes its honest report and exits with status 3. Input or runtime
failure exits with status 1; invalid syntax exits with status 2.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-performance-qualification
```

To run the retained real-daemon smoke after building `peritusd`:

```sh
PERITUS_H3_DAEMON="$PWD/target/debug/peritusd" \
  CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-performance-qualification \
  --test integrated_smoke -- --ignored
```

To run the one-command operator over a one-operation real campaign and verify its complete atomic
evidence bundle:

```sh
PERITUS_H3_DAEMON="$PWD/target/debug/peritusd" \
  CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-performance-qualification \
  --test campaign_evidence_smoke -- --ignored --test-threads=1
```

The focused production event acceptance test preserves the checked-in host profile, 120-second
duration, 500/s rate, seeded 8-KiB payload sizes, 32-worker/256-queue bounds, and 50-ms p99 objective
with at least 10,000 samples. It retains every arrival and latency plus a consistent SQLite backup,
verifies every successful event's exact stored bytes, and cleans up before asserting acceptance.
It runs no other workload, baseline comparison, or soak:

```sh
PERITUS_H3_DAEMON=/absolute/path/to/release/peritusd \
PERITUS_H3_SCRATCH=/existing/reviewed/nvme/directory \
PERITUS_H3_SOURCE_MANIFEST=/absolute/path/to/reviewed-sources.sha256 \
PERITUS_H3_ACCEPTANCE_OUTPUT=/existing/parent/new-event-acceptance \
CARGO_BUILD_JOBS=2 cargo test --locked --release -p peritus-performance-qualification \
  --test event_fidelity production_event_append_acceptance -- --ignored --test-threads=1
```

The output must not exist. A failed assertion is an acceptance failure with retained evidence, not
permission to change the objective or continue into optimization. Missing other H3 workloads and
the accepted baseline remain explicit in the full evaluator output.
