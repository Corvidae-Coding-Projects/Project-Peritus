# crates/app/peritus-daemon/src/outbox/qualification — navigation

## Responsibility

Implementation pieces for `qualification.rs`: Bounded public-admin qualification for an effect-before-ack outbox restart.

## Files in this folder

| File | Role |
| --- | --- |
| [blob.rs](blob.rs) | Real content-addressed artifact recovery on both sides of durable publication. |
| [effect.rs](effect.rs) | Exact identity-bearing filesystem destination for outbox crash qualification. |
| [gate.rs](gate.rs) | Real D1 gate-state recovery on both sides of its atomic C0 commit. |
| [journal_before.rs](journal_before.rs) | Real journal recovery when a staged append plan dies before durable commit. |
| [lease.rs](lease.rs) | Real lease persistence recovery on both sides of the journal commit. |
| [patch.rs](patch.rs) | Real patch transaction recovery on both sides of durable application. |
| [promotion.rs](promotion.rs) | Real F0 atomic promotion recovery on both sides of its C0 transaction. |
| [reboot.rs](reboot.rs) | Real-host reboot checkpoints around one durable C0 outbox delivery. |
| [snapshot.rs](snapshot.rs) | Real Git snapshot recovery on both sides of retained-reference publication. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [gate/](gate/AGENTS.md) | Implementation pieces for `gate.rs`: Real D1 gate-state recovery on both sides of its atomic C0 commit. |
| [snapshot/](snapshot/AGENTS.md) | Implementation pieces for `snapshot.rs`: Real Git snapshot recovery on both sides of retained-reference publication. |

Owning module: [qualification.rs](../qualification.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
