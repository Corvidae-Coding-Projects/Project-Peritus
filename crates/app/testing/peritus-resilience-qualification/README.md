# peritus-resilience-qualification

This crate owns the executable H1 release operator and the platform effect boundary used to
exercise a real Peritus release candidate. The runtime-neutral catalog, observations, invariants,
and verdict stay in `peritus-resilience`.

`peritus-h1` digests the exact candidate executable, stages that same executable into every fresh
subject, runs all 43 scenarios through the reviewed controller, and atomically publishes the full
JSON report. A report path is never overwritten.

```sh
peritus-h1 \
  --controller /reviewed/peritus-h1-controller \
  --candidate /release/peritusd \
  --scratch /private/h1/scratch \
  --artifacts /private/h1/artifacts \
  --report /evidence/h1-report.json \
  --subject-id peritus.release.candidate \
  --implementation "integrated Peritus release candidate"
```

The controller remains responsible for real daemon, storage, dependency, quota, VM, and reboot
effects. The operator refuses a descriptor whose build digest differs from the exact staged
candidate bytes.

The checked-in `peritus-h1-controller` currently owns the genuine
`h1.crash.journal.after-before-ack` route. It starts the exact staged daemon at the production
outbox qualifier, observes the durable effect-before-ack checkpoint, kills that process, restarts
the same candidate bytes, verifies exact effect reconciliation and live-fence settlement, retains
six independently digested evidence files, and proves cleanup. Other catalog routes return an
error until their real component or disposable-host control exists; they cannot inherit a fixture
result.

Use the explicit diagnostic option to exercise that route without presenting a one-case report as
production readiness:

```sh
CARGO_BUILD_JOBS=2 cargo build --locked \
  --package peritus-daemon --bin peritusd \
  --package peritus-resilience-qualification \
  --bin peritus-h1 --bin peritus-h1-controller

peritus-h1 \
  --controller target/debug/peritus-h1-controller \
  --candidate target/debug/peritusd \
  --scratch /private/h1/scratch \
  --artifacts /private/h1/artifacts \
  --report /evidence/h1-journal-diagnostic.json \
  --subject-id peritus.release.candidate \
  --implementation "integrated Peritus release candidate" \
  --diagnostic-scenario h1.crash.journal.after-before-ack
```

The report keeps the `custom` profile and `not-ready-custom-catalog` verdict even when the selected
case passes. The command exits successfully only to make focused qualification automation useful.

## Focused checks

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-resilience-qualification
CARGO_BUILD_JOBS=2 cargo clippy --locked --package peritus-resilience-qualification --all-targets --all-features -- -D warnings
```
