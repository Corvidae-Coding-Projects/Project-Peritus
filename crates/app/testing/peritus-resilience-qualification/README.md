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

The checked-in `peritus-h1-controller` currently owns fourteen genuine crash routes across the
journal, blob, retained Git snapshot, lease, patch, gate, and promotion commit boundaries. For
`h1.crash.journal.before`, the exact staged daemon builds a production append plan, publishes its
checkpoint before submission, and is killed; recovery requires an integrity-checked journal with
zero committed events, heads, outbox claims, or external effects. For
`h1.crash.journal.after-before-ack`, it is killed after the durable outbox effect checkpoint;
recovery requires exact effect reconciliation and live-fence settlement. Both routes retain six
independently digested evidence files and prove cleanup. Other catalog routes return an error until
their real component or disposable-host control exists; they cannot inherit a fixture result.

It also owns both blob commit routes through the production content-addressed artifact store. The
before case kills an exact owned writer while its complete bytes are still temporary and requires
restart recovery to remove them without publishing metadata or a reference. The after case kills
the daemon after publishing the verified object, durable metadata, and evidence-owned reference,
then requires all three to survive and agree after restart.

Both retained Git snapshot routes are real too. The before case holds a changed production
`CandidateTree` without calling snapshot creation and requires recovery to find no Peritus snapshot
ref or manifest. The after case publishes the deterministic synthetic commit and compare-and-swap
ref through `peritus-git`; recovery decodes the canonical manifest and reopens the exact commit,
tree, and ref while the controller independently checks the retained files.

Both lease routes use the real B1 lease reducer and journal commit adapter. The before case holds a
move-only commit request without submitting it and requires a completely empty lease history after
restart. The after case kills the staged daemon after the atomic event/projection commit and
requires a fresh process to recover the same request digest, projection revision and digest, and
producing event position.

Both patch routes use the real workspace-bound plan and recoverable filesystem transaction adapter.
The before case holds the checked plan without creating transaction metadata or target bytes. The
after case retains the exact applied receipt while a fresh process and the controller independently
re-hash the postimage and require an empty transaction directory.

Both gate routes use a real contract-bound D1 `GatePlan`, accepted start transition, and C0 commit
adapter. The before case retains the accepted transition only in the killed process. The after case
atomically commits its gate event and complete state checkpoint. Fresh recovery rebuilds the exact
successor from the journal and independently checks the plan, semantic-state, checkpoint, revision,
and producing-position digests.

Both promotion routes build the real F0 campaign and production-pointer predecessors, finalized
artifact evidence, and signed approve-once decision. The before case holds only the accepted final
transitions when killed. The after case commits both activation events, both checkpoints, and
approval consumption in one C0 transaction. Fresh recovery checks the exact proposal,
authorization, campaign, pointer, approval revision, event count, and aggregate-head count.

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
Use `h1.crash.journal.before` in the last argument to run the other journal boundary.
The equivalent blob diagnostics are `h1.crash.blob.before` and
`h1.crash.blob.after-before-ack`. The snapshot diagnostics are `h1.crash.snapshot.before` and
`h1.crash.snapshot.after-before-ack`. The lease diagnostics are `h1.crash.lease.before` and
`h1.crash.lease.after-before-ack`. The patch diagnostics are `h1.crash.patch.before` and
`h1.crash.patch.after-before-ack`. The gate diagnostics are `h1.crash.gate.before` and
`h1.crash.gate.after-before-ack`. The promotion diagnostics are
`h1.crash.promotion.before` and `h1.crash.promotion.after-before-ack`.

## Focused checks

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-resilience-qualification
CARGO_BUILD_JOBS=2 cargo clippy --locked --package peritus-resilience-qualification --all-targets --all-features -- -D warnings
```
