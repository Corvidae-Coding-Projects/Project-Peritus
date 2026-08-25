# peritus-review

`peritus-review` is the D2 durable review-cycle and finding-lifecycle engine. It binds every
review to an immutable acceptance contract and exact candidate revision, validates bounded
structured submissions, enforces each quorum and independence dimension, conserves findings until
an authorized current disposition exists, and replays its complete state through C0.

The crate accepts inert reviewer, fixer, and external authority observations. It does not invoke
providers, execute tools, mutate workspaces, issue waivers, or accept a run.

## Boundary

The checked `ReviewBinding` fixes the B2 contract/review-policy snapshot, exact `RevisionTuple`,
candidate and tree digests, and producer provenance. Assignments additionally bind a stable cycle,
canonical category set, `ReviewerIdentity`, and exact C6 context-plan/freshness facts. Any binding
drift makes earlier material historical rather than current.

The reducer owns the closed lifecycle for:

- reviewer assignment, submission, invalidation, and cancellation;
- stable findings and provenance-preserving duplicate reconciliation;
- fixer responses and independent reviewer confirmations;
- waiver requests and consumption of already-authorized B1/B2 waiver observations;
- revision advance, budget/cycle/oscillation escalation, failure, cancellation, and completion.

`Completed` means D2 review completion only. B0/E0 remains the overall run-acceptance boundary.

## Durable protocol

D2 uses B3 schema-version-one families 53–55 for commands, events, and complete state. The C0
aggregate is `AggregateKind::Review` (tag 9); checkpoint namespace `0xD201` atomically couples each
accepted event with its successor state under aggregate and state compare-and-swap. Restart always
replays from genesis and checks the installed checkpoint for exact equivalence.

The codec and durability modules are narrow H-class boundaries. The domain reducer, bounds,
freshness, quorum, conservation, terminal, oscillation, and replay witnesses remain executable
Verus Rust and ordinary safe Rust.

## Verification

Run focused checks serially on a constrained host:

```text
CARGO_BUILD_JOBS=1 cargo test --locked --package peritus-review --all-targets --all-features
CARGO_BUILD_JOBS=1 cargo clippy --locked --package peritus-review --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=1 RUSTDOCFLAGS="-D warnings" cargo doc --locked --package peritus-review --all-features --no-deps
CARGO_BUILD_JOBS=1 cargo verus verify --locked --package peritus-review --all-features --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

See [`docs/d2-review-engine.md`](../../../docs/d2-review-engine.md) for the operating contract.
