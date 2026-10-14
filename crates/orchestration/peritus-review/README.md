# peritus-review

## Folder guide

Durable deterministic review cycles, finding lifecycle, quorum, and reconciliation.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-review`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic review cycles and finding lifecycle for Peritus. |

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

The production-facing finding ledger admits normalized typed categories without a reviewer-owned
blocking Boolean. Policy always blocks unresolved correctness, requested-behavior, build-coverage,
test-coverage, and security findings. Omission alone cannot close a finding: a fixer proposal and a
later fresh reviewer confirmation are both required, and the complete ledger supports checked
durable restoration.

## Durable protocol

D2 uses B3 schema-version-one families 53–55 for commands, events, and complete state. The C0
aggregate is `AggregateKind::Review` (tag 9); checkpoint namespace `0xD201` atomically couples each
accepted event with its successor state under aggregate and state compare-and-swap. Restart always
replays from genesis and checks the installed checkpoint for exact equivalence.

The codec and durability modules are narrow H-class boundaries. The domain reducer, bounds,
freshness, quorum, conservation, terminal, oscillation, and replay witnesses remain executable
Verus Rust and ordinary safe Rust.

## Historical background



See [`docs/d2-review-engine.md`](../../../docs/d2-review-engine.md) for the operating contract.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
