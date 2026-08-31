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

The checked-in `peritus-h1-controller` currently owns 40 genuine routes across the journal,
blob, retained Git snapshot, lease, patch, gate, and promotion commit boundaries, plus active
projection repair, journal/blob/snapshot/acceptance-evidence/harness-promotion corruption controls, and
provider/tool/worker dependency failure. It also owns all eleven active E0 lifecycle phases. Each
lifecycle case builds the shortest legal production reducer prefix, commits every command and state
checkpoint through C0, kills the exact staged `peritusd`, and requires a fresh process to replay
byte-identical state and authoritative ownership. For
`h1.crash.journal.before`, the exact staged daemon builds a production append plan, publishes its
checkpoint before submission, and is killed; recovery requires an integrity-checked journal with
zero committed events, heads, outbox claims, or external effects. For
`h1.crash.journal.after-before-ack`, it is killed after the durable outbox effect checkpoint;
recovery requires exact effect reconciliation and live-fence settlement. Both routes retain six
independently digested evidence files and prove cleanup. Other catalog routes return an error until
their real component or disposable-host control exists; they cannot inherit a fixture result.

The artifact-finalization disk route uses the production store's checked logical quota. It opens
two exact writers while the quota can admit either one, finalizes and references the first, and
requires the second finalization to be rejected by durable catalog accounting after publication.
The store removes those losing bytes before returning the typed quota error. A fresh staged daemon
then verifies the admitted object, exact used-byte count, empty temporary namespace, absent rejected
metadata, and healthy journal.

The journal-append disk route opens the production SQLite journal, fixes that connection's page
ceiling at its current allocation, and submits one oversized exact append through the ordinary
transaction path. The stage must receive SQLite's real storage-exhaustion result and prove that the
command, event, and aggregate head are absent. A separate candidate process then reopens the same
database and independently verifies the rejected append left no partial authoritative state.

The snapshot-manifest disk route first fills a valid production artifact-store quota with one
referenced object, then creates an exact production Git snapshot and attempts to finalize its
workspace manifest through the shared publication boundary. Quota rejection must release the
unpublished retained ref before returning. A fresh candidate process independently verifies that
the ref is absent, the admitted object remains exact and referenced, the temporary namespace is
empty, and the journal is healthy.

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

The snapshot-corruption route publishes the same production manifest and retained ref, redirects
that ref to a different commit, and proves a fresh process refuses to reopen it. Recovery uses the
production Git adapter to atomically remove the bad ref from active use, preserve its observed
value under the quarantine namespace, and return the same result when repeated.

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

The projection-corruption route uses the production startup projection catalog. It changes the
exact active payload while retaining its recorded digest, proves that the generation is invalid,
and starts a fresh daemon process. Recovery must install a new generation whose payload and digest
match a genesis replay, leave the authoritative empty journal unchanged, and then become reusable.

The journal-corruption route first commits a real D1 gate event and state checkpoint. It changes
the stored event frame without updating its recorded digest, then invokes the complete production
startup path in a fresh process. Startup must report typed corrupt state before allocating an
authority epoch or application principal, and the controller proves the corrupt bytes and every
authoritative row count remain unchanged.

The blob-corruption route publishes a real content-addressed object and durable evidence reference,
then changes its active bytes without changing the recorded identity. Fresh store startup must
persist the corrupt integrity state, move the divergent bytes to quarantine, retain the audit
reference, and make the artifact unavailable to readers or new references. A second restart must
preserve that contained state.

The acceptance-evidence corruption route admits a real revision-bound portable evidence record,
then changes its stored record bytes without changing any indexed identity. Fresh evidence-store
startup must copy every raw indexed field and the corrupt bytes into a digest-bound durable
quarantine before the record can be read. The original evidence identity remains unavailable, the
authoritative journal remains unchanged, and a second startup must preserve exactly one identical
quarantine observation.

The harness-promotion corruption route prepares and commits the real F0 campaign, production
pointer, and approve-once activation transaction, then publishes both resulting evolution outbox
directives through the production evidence boundary. It changes only the durable
harness-activation evidence bytes. A fresh evidence-store startup must quarantine that exact row
while keeping the campaign evidence readable, the complete 16-event/four-head journal unchanged,
and the production pointer replayable at the promoted revision. Repeating startup must preserve one
identical quarantine observation.

The six dependency routes use the real durable scheduler and one real effect boundary per
dependency. Provider cases execute the staged daemon through the executable-backed provider
transport. Tool cases use the same grounded, receipt-backed `run_command` path as ordinary coding
runs. Worker cases abort a task owned by the daemon worker supervisor. A death case leaves one
retryable failure for fresh replay to requeue. A retry-exhaustion case executes the exact configured
attempt ceiling and requires fresh replay to retain terminal exhausted non-success. All six focused
diagnostics passed under
`/home/doll/.local/state/peritus/qualification/h1/dependency-routes.5nwGZV`.

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
The projection diagnostic is `h1.corruption.projection`.
The journal diagnostic is `h1.corruption.journal`.
The blob diagnostic is `h1.corruption.blob`.
The snapshot diagnostic is `h1.corruption.snapshot`.
The acceptance-evidence diagnostic is `h1.corruption.acceptance-evidence`.
The harness-promotion evidence diagnostic is `h1.corruption.harness-promotion`.
The dependency-death diagnostics are `h1.death.provider`, `h1.death.tool`, and
`h1.death.worker`. The exhaustion diagnostics are `h1.retry-exhaustion.provider`,
`h1.retry-exhaustion.tool`, and `h1.retry-exhaustion.worker`.
The artifact quota diagnostic is `h1.disk-full.blob-finalize`.
The journal quota diagnostic is `h1.disk-full.journal-append`.
The snapshot-manifest quota diagnostic is `h1.disk-full.snapshot-commit`.
The daemon lifecycle diagnostics use the `h1.daemon-kill.` prefix followed by `writer-pending`,
`writer-active`, `gates-pending`, `gates-active`, `review-pending`, `review-active`,
`fixer-pending`, `fixer-active`, `revision-advancing`, `evaluating-acceptance`, or
`kernel-acceptance-pending`.

## Focused checks

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-resilience-qualification
CARGO_BUILD_JOBS=2 cargo clippy --locked --package peritus-resilience-qualification --all-targets --all-features -- -D warnings
```
