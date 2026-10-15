# crates/analysis/peritus-debugger/src/durability/commit — navigation

## Responsibility

Implementation pieces for `commit.rs`: Atomic family-83 event, family-84 checkpoint, artifact, and outbox persistence.

## Files in this folder

| File | Role |
| --- | --- |
| [claim.rs](claim.rs) | Commit-mode validation and claim-bound idempotency digests. |
| [outbox.rs](outbox.rs) | Stable outbox planning and finalized-artifact dependencies. |

Owning module: [commit.rs](../commit.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
