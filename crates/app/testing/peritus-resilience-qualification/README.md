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

## Focused checks

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-resilience-qualification
CARGO_BUILD_JOBS=2 cargo clippy --locked --package peritus-resilience-qualification --all-targets --all-features -- -D warnings
```
