# F0 production harness evolution

`peritus-evolution` is Project Peritus's evidence-backed authority for selecting and activating a
new production harness revision. It connects immutable E1 materialization, E2 diagnosis, E3
evaluation, D2 review, B0 dispatch, and B1 human approval without allowing any report, score, or
model proposal to promote itself.

F0 is local-first and headless. G0 will later compose it into a service and G1/G2 will expose its
operator surfaces. The crate itself contains no provider, process, shell, network, filesystem, or
raw-SQL capability.

## Durable authorities

F0 deliberately owns two C0 aggregates:

| Authority | Aggregate key | Tag | Wire families | State namespace |
|---|---|---:|---|---:|
| evolution campaign | `EvolutionCampaignId` | 16 | 88 command, 89 event, 90 state | `0xF001` |
| production harness | `ProjectId` | 17 | 91 command, 92 event, 93 state | `0xF002` |

A campaign terminates after promotion, rejection, failure, or cancellation. The production pointer
lives across every campaign and serializes activations for one project. This separation allows
multiple campaigns to analyze the same baseline while preserving one project-global compare-and-
swap at promotion time.

Journal schema 9 admits both aggregate kinds. Migration v9 preserves every schema-8 row, frame,
position, digest, and hash before widening the aggregate-kind checks. Older F0 schemas are decoded
only by their registered version; unknown frames remain inert.

## Exact inputs

An installed production binding contains both the shared `RevisionTuple` and E1's full
`HarnessRevisionIdentity`, plus the materialization receipt and installed snapshot digests. A
campaign freezes that exact baseline and a typed promotion policy bound to the protected E1
`EvolutionStrategy` component.

F0 captures diagnosis and evaluation data only from live validated and published E2/E3 values.
The capture produces an F0-owned canonical summary carrying all fields required after restart:
source identity, report/profile digest, artifact and evidence identity, journal position, exact
baseline/candidate arm, dataset and evaluator bindings, analysis digest, and cited diagnostic facts.
Wire decoding of those summaries is private replay machinery; callers cannot manufacture a
published-evidence value from unrelated digests.

Executable candidate changes also capture a terminal D2 review. The review must be completed for
the exact candidate, have the required independent quorum, and conserve every finding. Review
evidence is a typed binding, never a `reviewed: bool` shortcut.

## Change manifests and variants

Every change manifest is immutable, bounded, content addressed, and citation complete. It records:

- the exact component before/after identities and content or executable digests;
- its hypothesis and considered alternatives;
- predicted fixes, regressions, safety and resource effects;
- falsification criteria and compatibility impact; and
- a retained rollback target.

F0 resolves the declared deltas against the exact E1 baseline and candidate graphs. Undeclared,
omitted, equal, or mismatched deltas reject. Ordinary campaigns cannot address protected security
roots, B1 human authority, sealed datasets/evaluators, trust-boundary definitions, or the protected
promotion policy.

A variant binds one materialized E1 candidate and a canonical nonempty manifest set. A multi-change
variant names an interaction group. Its observed result belongs to that group unless separately
evaluated isolated variants justify per-change attribution.

## Attribution and selection

F0 consumes E3's frozen integer and fixed-point observations. It does not recalculate statistics or
accept caller-provided aggregate scores. Each prediction becomes exactly one of `Confirmed`,
`Contradicted`, `Inconclusive`, or `NotObserved`, retaining the observation or explicit
unavailability that produced the verdict.

Promotion policy evaluates independent criteria for correctness lower bounds, task and critical
regressions, safety, stability, reliability, cost, latency, trace and teardown completeness,
review, schema compatibility, and attribution coverage. Mandatory criteria are conjunctive:
`Failed` or `Unavailable` always rejects. Favorable cost or correctness cannot offset failed safety,
contamination, missing evidence, or a critical regression.

Eligible candidates are ranked by the policy's frozen lexicographic objective vector and stable
variant identity. Ordering never uses floating point, map iteration, wall-clock time, insertion
order, or platform path behavior.

## Promotion authority

A promotion proposal binds the project, campaign, current and candidate pointer, change, variant,
attribution, evaluation, review, policy, rollback target, and evidence-bundle digests. F0 then
requires all of the following for that exact action:

1. a B0-dispatched `HarnessPromotion` action;
2. a durably committed matching B1 capability use;
3. current credential-registry and authority-epoch facts; and
4. a matching approve-once B1 human decision.

Activation uses C0's durable approval-use adapter. One SQLite transaction checks both aggregate
heads and writes the campaign terminal event/checkpoint, pointer activation event/checkpoint,
activation history, approval consumption, exact artifact dependencies, and optional downstream
notification. Before the commit none of those facts exists; after it all of them exist. Approval
cannot remain reusable after a successful pointer change.

The pointer transition is authoritative journal state. An outbox message such as a future
`production-harness.changed` notification is only retryable downstream observation and never the
activation itself.

## Rollback

Rollback is a new activation, not history rewriting. Its target must be a retained previously active
E1 revision, remain schema compatible, differ from the current pointer, and receive a fresh exact
B0/B1 authorization. The new pointer record cites the activation being reversed while preserving
both histories.

Runs retain the governing harness binding captured when they were created. A later promotion or
rollback does not reinterpret or mutate an existing run.

## Recovery operations

Recovery begins from C0 journal truth:

1. replay each campaign's contiguous family-89 event chain and compare its complete family-90
   checkpoint;
2. replay each project's family-92 pointer chain and compare its family-93 checkpoint;
3. quarantine any head/checkpoint, digest, sequence, or prior-pointer disagreement;
4. verify every referenced content-addressed artifact;
5. redeliver unacknowledged publication or notification outbox rows; and
6. treat an already committed exact command as idempotent success only when its complete events and
   checkpoints match.

A crash before activation commit leaves the prior pointer and unused approval. A crash after commit
leaves the new pointer and consumed approval even if publication or notification has not settled.
Retry must use the same composite command digest; a different action is a conflict, not a retry.

Typical recovery guidance is:

| Failure | Operator action |
|---|---|
| stale campaign baseline or pointer generation | refresh state and create a successor campaign |
| missing or unavailable mandatory evidence | obtain complete evidence; do not waive it as a score |
| denied, expired, or mismatched authority | request a new exact B1 approval |
| unresolved artifact/publication directive | verify the artifact and reconcile the exact outbox claim |
| malformed or unsupported schema | quarantine the frame and upgrade with a registered migration |
| checkpoint or hash-chain disagreement | stop mutation, export integrity evidence, and restore/replay |
| incompatible rollback target | choose another retained compatible revision or materialize a successor |

## Verification

Focused F0 development uses one build job and one heavy command at a time:

```console
CARGO_BUILD_JOBS=1 cargo test --package peritus-evolution --all-targets --all-features --locked -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --package peritus-evolution --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=1 cargo test --package peritus-conformance --all-targets --all-features --locked -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --package peritus-migrations --all-targets --all-features --locked -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-evolution --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

The complete pre-merge evidence is one serialized `CARGO_BUILD_JOBS=1 just gate-a`, followed by the
required Linux, macOS, Windows, Foundation, and Verus hosted checks. A report is not production
authority, a local pass is not hosted evidence, and the F0 PR remains unmerged until every required
runner is green.
