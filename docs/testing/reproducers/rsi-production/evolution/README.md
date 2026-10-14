# Real E1/E2/D2/F0 owner replay

## Folder guide

peritus-rsi-evolution-replay package.

Standalone replay/consumer package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.lock](Cargo.lock) | Locked dependency resolution for the neighboring package manifest. |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-rsi-evolution-replay`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [activation.rs](activation.rs) | Real F0 host authority adapter for the disposable native fixture only. Fresh explicit CLI approval input is required separately for promotion and rollback. |
| [e1.rs](e1.rs) | Real E1 materialization producer. No fabricated receipt, publication, or approval constructors. |
| [f0.rs](f0.rs) | Actual owner composition over the disposable native E1/E2/E3 fixture. |
| [observed-owner-results.json](observed-owner-results.json) | Structured observed owner results data with fields `schema`, `note`, `materialization`, `independent_review`, `selection`. |
| [publication.rs](publication.rs) | Read back actual published F0 owner evidence; never claim or acknowledge an outbox row here. |
| [review.rs](review.rs) | Imports an independently executed review into its actual D2 owner. This consumer accepts only the fresh delegated review's final scoped result. It never manufactures a review, provider identity, finding, or approval. |

These standalone programs operate the real public owner APIs and ordinary SQLite, artifact,
Git/worktree, C1, and approval services. They do not generate passing review receipts or invoke
unit, qualification, or formal suites. Use a fresh disposable fixture and actual baseline/candidate
Peritus executables. Never point the activation driver at a live daemon's state directory.

Build with the repository's pinned toolchain and a bounded job count:

```sh
CARGO_TARGET_DIR="$REPO/target" cargo build --release -j 2 --manifest-path "$REPO/docs/testing/reproducers/rsi-production/evolution/Cargo.toml"
"$REPO/target/release/peritus-rsi-evolution-real-consumer" "$FRESH_FIXTURE" "$BASELINE_ELF" "$CANDIDATE_ELF"
```

The fixture producer creates real Git worktrees, exact E1 materializations and C1 authorizations,
an initialized baseline production pointer with immutable evidence, and an Active D2 review run.
It emits `actual-materializations.json` with `fixture_only: true`. The two executable digests must
remain declared by the actual E1 graph and preserved in the artifact store.

Run the adjacent native daemon consumer against a copy of that fixture to obtain actual Published
E3 evaluation. Use its broad CLI campaign for deny-wins behavior and its focused version campaign
for a narrowly scoped positive comparison. Freeze the policy before execution. The focused case
proves CLI version mechanics; it is not evidence of general model capability.

With every daemon writer closed, run:

```sh
"$REPO/target/release/peritus-rsi-f0-real-consumer" stage "$FIXTURE" "$ACTUAL_E3_CAMPAIGN"
```

This produces a real observation-only E2 ReportReady job from actual C0/C7 terminal artifacts. The
host initializes its protected `rsi` capsule directory even when an infrastructure failure occurred
before any C2 child started. Reopen the ordinary native daemon to publish that job through its real
outbox claim, then close it before the next SDK operation.

Obtain a fresh independent review of the exact materialized candidate, source/binary identities,
sealed contract, and real raw results. Preserve the reviewer's actual response and the exact
canonical UTF-8 input capsule. The review driver accepts only the declared final PASS result, exact
required categories and binding, and no unresolved findings; it rejects a pending response. It
records an externally attested review context, not a C5 request transcript. Unavailable provider
and model metadata stay null, with explicit zero unavailable sentinels in the legacy identity
fields; this single-reviewer replay makes no provider or model diversity claim.

```sh
"$REPO/target/release/peritus-rsi-f0-real-consumer" admit "$FIXTURE" "$ACTUAL_E3_CAMPAIGN"
"$REPO/target/release/peritus-rsi-review-real-consumer" "$FIXTURE" "$ACTUAL_REVIEW_JSON" "$EXACT_REVIEW_INPUT_JSON"
"$REPO/target/release/peritus-rsi-f0-real-consumer" select "$FIXTURE" "$ACTUAL_E3_CAMPAIGN"
"$REPO/target/release/peritus-rsi-activation-real-consumer" prepare "$FIXTURE" "$ACTUAL_E3_CAMPAIGN"
```

Selection and proposal preparation must leave production unchanged. The broad campaign should
reject under the retained policy; the focused positive campaign may enter PromotionReview. The
approval driver requires a separate explicit operation naming the exact prepared proposal digest:

```sh
"$REPO/target/release/peritus-rsi-activation-real-consumer" approve-promotion-once "$FIXTURE" "$ACTUAL_E3_CAMPAIGN" "$EXACT_PROMOTION_DIGEST"
"$REPO/target/release/peritus-rsi-activation-real-consumer" prepare-rollback "$FIXTURE" "$ACTUAL_E3_CAMPAIGN"
"$REPO/target/release/peritus-rsi-activation-real-consumer" approve-rollback-once "$FIXTURE" "$ACTUAL_E3_CAMPAIGN" "$EXACT_ROLLBACK_DIGEST"
```

This disposable host adapter creates actual B0 dispatch, B1 capability and public credential
registry owners, signs the exact owner-provided approval preimage using an ephemeral key, and
consumes approve-once authority in the existing atomic activation transaction. It also exercises
invalid signature, wrong action, wrong project/revision, and reused-consumption rejection. No
private key is written. Its pure B2 constructor is the existing production command contract source;
its ID must equal the actual prepared target's acceptance ID.

Read the emitted public credential payload and generation metadata on later native restarts.
Between promotion and rollback, use the native consumer's `current_program` pathway to invoke
production through genuine C2/E3. Verify baseline output before promotion, candidate output after
promotion, unchanged retained pre-promotion binding, and baseline output after separately approved
rollback. Pointer reads alone do not establish adoption.

After the ordinary daemon has published the terminal decision and activation outboxes, close it
and check their exact retained C0 rows and public evidence records:

```sh
"$REPO/target/release/peritus-rsi-publication-real-consumer" verify "$FIXTURE" "$ACTUAL_E3_CAMPAIGN"
```

The publication consumer reads existing acknowledged rows and verified evidence, rejects an
unrelated real baseline receipt without appending, and records the terminal campaign's actual
publication acknowledgement. Retrying that exact acknowledgement must leave the journal unchanged.

The consumers retain actual identities, hashes, exact approval decisions, negative errors, and
readback results in their owned fixture. Large copied repositories, executables, private keys, and
live state are not bundled here. The campaign report records the observed results and remaining
scope explicitly.
