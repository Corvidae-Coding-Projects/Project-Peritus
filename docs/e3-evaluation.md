# E3 evaluation

`peritus-eval` is Project Peritus's durable, reproducible harness-evaluation boundary. It compares
one immutable baseline E1 harness revision with one immutable candidate revision over an exact
dataset, provider profile, execution contract, metric policy, and rollout plan. It retains the
complete result ledger and publishes a canonical, evidence-backed report.

E3 is deliberately non-authoritative. It cannot modify either harness, write a workspace, grant a
capability, waive a review finding, accept a delivery, select a winner, promote a revision, or move
a production pointer. F0 may later consume an E3 report as inert evidence under a separately
authorized evolution policy.

## Dataset and evaluator isolation

An evaluation begins with a checked `DatasetManifest`. Each task has a stable identity, declared
partition, positive weight, resource ceiling, public `CandidateTaskInput`, and separate
`SealedEvaluatorInput`. Candidate-visible values cannot contain expected answers or verifier
roots. Evaluator material remains an opaque artifact binding until the evaluator stage.

The checker rejects:

- empty manifests, duplicate task identities, undeclared partitions, or zero weights;
- artifact reuse across candidate, evaluator, verifier, sandbox-image, and environment roles;
- malformed or noncanonical ordering; and
- any task or aggregate count that exceeds the compiled `EvaluationLimits`.

The checked manifest has a domain-separated canonical digest. Loading is inert: E3 records exact
content-addressed roots and has no repository-relative path, URL, credential, or executable field.

## Frozen profile

`FrozenEvaluationProfile` binds every input that could change an outcome:

- the checked dataset and selected partitions;
- distinct baseline and candidate E1 revision identities from one lineage, unless the profile
  explicitly declares a cross-lineage comparison;
- exact E1 materialization receipt digests for both arms;
- the complete C5 provider/model/control snapshot, including retry and seed-delivery semantics;
- C2/C3 sandbox plan, backend admission, environment, image, deadline, concurrency, resource,
  restricted-isolation, and teardown requirements;
- rollout multiplicity and deterministic seed policy;
- correctness, pass-at-k, paired, bootstrap, stability, distribution, and resource policies; and
- the declared treatment of infrastructure failures and missing resource observations.

Construction validates all bindings before producing the profile digest. Changing any field
creates a different profile and prevents old plans or results from being admitted. Provider and
execution values remain fixed-width or integer protocol values; floating-point platform behavior
is not part of the durable identity.

## Deterministic planning and scheduling

`EvaluationPlan::build` expands each selected task into the complete baseline/candidate rollout
matrix. Paired cells share the same task, ordinal, and deterministic seed. Rollout IDs, D3 work
IDs, request digests, batch digests, and the plan root derive from the complete semantic input and
are independent of map iteration, task wakeups, or wall-clock timing.

Plan storage is canonical. Dispatch order is a separate deterministic D3 concern, so scheduling
fairness cannot rewrite statistical identity. E3 emits bounded
`peritus.eval.schedule-rollout.v1` directives containing existing D3 `WorkSpec` values. It does
not introduce an internal queue or duplicate D3 reservations, capacity accounting, worker-loss
handling, cancellation, or retry ownership.

Every external step is commit-before-effect:

1. Commit the schedule request and its stable outbox directive.
2. Claim and settle that exact directive with the D3 acknowledgement.
3. Commit the execution request and claim before invoking the runtime-owned execution adapter.
4. Commit the terminal record and acknowledge the exact execution claim in one C0 transaction.

Retries preserve task, arm, ordinal, seed, profile, request, and logical rollout identity. Exact
duplicates are idempotent. Conflicting results are rejected rather than silently choosing one.

## Execution boundary

`RolloutExecutionPort` is the checked seam used by the later application composition layer. E3
constructs inert requests containing exact C2/C3 isolation, workspace, environment, resource,
deadline, teardown, provider, seed, and request bindings. The owner of the port performs the real
C2/C3 and C5 work and returns E3-owned observations.

Candidate execution receives only public task material. The evaluator can run only after a
candidate artifact is finalized and receives the sealed evaluator binding separately. E3 checks
the returned rollout, attempt, request, provider, execution, output, and resource identities before
creating a result. A candidate failure does not run the evaluator. An evaluator outage is an
infrastructure failure, never a task failure.

`RolloutOutcome` distinguishes task pass, task failure, infrastructure failure, ambiguous result,
and cancellation. A valid evaluator verdict is the sole source of task pass/failure. D3 execution
success means only that the external work completed; it does not imply that the evaluated task
passed.

## Complete accounting

`RolloutLedger` is constructed from the frozen plan and therefore knows every expected logical
rollout before execution. It retains bounded attempts and admits at most one terminal record per
rollout. An exact duplicate terminal is idempotent; a conflicting terminal is a binding failure.

The conservation identity is:

```text
expected = passed + task_failed + infrastructure_failed + ambiguous + cancelled
```

Analysis cannot begin until this identity holds. Cancellation is durable and terminal. Planned
work can cancel locally, while scheduled or running work must settle its existing schedule or
execution claim with an exact cancellation observation. E3 reuses that claim instead of creating
a second competing outbox message. Late success cannot resurrect a cancelled rollout or campaign.

Resource observations retain elapsed microseconds, provider input/output tokens, cost microunits,
memory high-water bytes, CPU microseconds, process high-water count, trace completeness, and
teardown completeness. Missing values remain missing. Checked addition rejects overflow, and the
frozen policy decides whether a metric becomes unavailable or a declared infrastructure treatment
contributes a failure denominator.

## Statistical analysis and reports

The analysis layer derives deterministic, integer/fixed-point records from the complete ledger:

- raw correctness counts and frozen Wilson-95 intervals;
- exact combinatorial pass-at-k where its declared preconditions hold;
- paired pass/pass, pass/fail, fail/pass, and fail/fail conservation;
- deterministic hash-seeded paired bootstrap and sign diagnostics;
- per-task stability and transition observations;
- latency, token, cost, memory, CPU, and process distributions with raw count, missing count, sum,
  extrema, integer mean, and selected fixed quantiles; and
- separate reliability counts for task, infrastructure, ambiguous, cancelled, trace-incomplete,
  teardown-incomplete, and retry outcomes.

Invalid denominators, incompatible pairing, incomplete cells, arithmetic overflow, insufficient
samples, and missing-required observations produce explicit unavailability or errors according to
the frozen policy. They never become zero, success, or an omitted row.

`EvaluationReport` binds the dataset, profile, plan, complete analysis, and optional constraint
observations. Validation reruns all conservation, compatibility, bounds, and non-authority checks
before canonical bytes can be published. The report contains no promote, activate, rollback,
accept, waive, patch, capability, or production-pointer operation.

## Durable protocol and replay

The stable version-one identities are:

| Purpose | Stable identity |
|---|---|
| evaluation command frame | B3 family 85, schema 1 |
| evaluation event frame | B3 family 86, schema 1 |
| complete evaluation checkpoint | B3 family 87, schema 1 |
| C0 aggregate kind | `Evaluation`, tag 15 |
| C0 checkpoint namespace | `0xE301` |

Decoded frames are inert. Checked constructors and the pure reducer revalidate phase, sequence,
predecessor, prior-state digest, profile binding, batch ordering, rollout identity, terminal
dominance, and canonical state before accepting a transition. Unknown tags, unsupported versions,
invalid lengths, truncation, noncanonical values, and trailing bytes reject.

Every transition appends one family-86 event and stores one complete family-87 checkpoint in the
same journal transaction. `load_evaluation_replay` validates the C0 chain and folds it through the
same reducer, then compares the rebuilt state with the checkpoint. Recovery chooses among
redelivery, analysis, artifact reconciliation, publication retry, evidence settlement,
cancellation continuation, completion, and quarantine using durable observations; it never guesses
that an external effect succeeded.

## Report publication

Publication keeps report identity and effect ordering explicit:

1. Analyze only a complete ledger and build a validated canonical report.
2. Finalize the exact report bytes in the content-addressed C0 artifact store.
3. Commit `CompleteReport` with that artifact dependency and one stable
   `peritus.eval.publish-report.v1` directive.
4. Claim the directive, verify the artifact, and admit provenance-bound `evaluation-report`
   evidence against a checked journal integrity export.
5. Commit `RecordPublication` and acknowledge the exact claim fence atomically.

A restart reuses the same content digest, evidence identity, directive identity, and command
fences. A crash cannot manufacture a second logical report or make unreferenced staged bytes
authoritative. Conflicting artifact, evidence, report-position, or claim identity is quarantined.

The rebuildable `EvaluationProjection` exposes only bounded phase, plan, rollout counts, analysis,
report, publication, cancellation, and safe failure state. It carries no candidate/evaluator
payload, credential, capability, or mutation method.

## Schema migration

C0 schema version 8 widens the constrained journal aggregate-kind columns from tags 1–14 to 1–15.
The backup-required migration copies historical heads and events in canonical order, verifies
counts and integrity, rebuilds the command index, and publishes schema/user version 8 in one
transaction.

The frozen v7 fixture proves that every historical tag and frame remains byte-exact. The upgrade
test appends tag-15 evaluation data, runs the journal integrity scanner, restores the whole-file
backup, and confirms the original v7 rows and version. Once tag-15 data exists, use forward repair
or the verified backup rather than opening the store with an older binary.

## Verification and operation

A2's thirteen-case evaluation catalog covers frozen inputs, candidate/evaluator isolation,
deterministic planning, complete accounting, statistics, infrastructure classification,
cancellation, replay, malformed frames, publication, redaction, panic containment, and teardown
isolation. The production bridge runs all thirteen cases against actual E3 types and behavior.

Verus proves executable refinements for accounting conservation, pass-at-k preconditions, terminal
dominance, frozen-profile validity, ledger validity, statistical prerequisites, legal transition
facts, replay facts, and report non-authority. These proofs do not claim C0 I/O, cryptographic hash,
codec, provider, scheduler, or process behavior; those boundaries have executable integration and
compatibility tests.

Run Cargo, Verus, xtask, and `just` commands serially with `CARGO_BUILD_JOBS=1`. The merge authority
is one complete local Gate A followed by hosted Gate A and Foundation matrices on Linux, macOS, and
Windows.

Operationally:

- preserve campaign, profile, plan, command, rollout, work, and outbox identities during retry;
- treat missing or ambiguous results as explicit incomplete evidence, never as task failure;
- reconcile C0 state before redispatching any schedule, execution, cancellation, or publication;
- retain the canonical report and its raw counts instead of recomputing under a changed policy;
- keep sealed evaluator artifacts outside candidate-visible requests and default diagnostics; and
- route any selection or production change through F0 and B1 authority, never through E3.
