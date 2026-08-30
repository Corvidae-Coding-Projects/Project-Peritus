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

Long campaigns use deterministic reservoir sampling per workload and metric. Objective metrics
retain twice their required sample count and diagnostics retain a bounded representative set, then
the coordinator merges everything into one monotonic campaign sequence. Queue workloads end with
an exact drain when their operation count stops partway through a saturation cycle, so successful
plans return the shared ledger to a balanced terminal state.

`CampaignEvidenceWriter` publishes a completed campaign through a private temporary directory and
one final rename. It refuses an existing destination, reparses the exact profile, workload, and
optional baseline documents, and requires them to equal the typed inputs that were executed. It
streams copies of `peritusd` and the qualification runner while recomputing their recorded SHA-256
identities, then retains measurements, receipts, accounting, machine facts, the content-addressed
manifest, and its bound report. A failure never creates the requested final bundle path.

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

To run a one-operation real campaign and verify its complete atomic evidence bundle:

```sh
PERITUS_H3_DAEMON="$PWD/target/debug/peritusd" \
  CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-performance-qualification \
  --test campaign_evidence_smoke -- --ignored --test-threads=1
```
