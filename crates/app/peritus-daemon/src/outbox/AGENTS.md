# crates/app/peritus-daemon/src/outbox — navigation

## Responsibility

Implementation pieces for `outbox.rs`: Durable C0 outbox delivery supervision.

## Files in this folder

| File | Role |
| --- | --- |
| [claims.rs](claims.rs) | Typed, effect-free decoding of claimed C0 outbox rows. |
| [clock.rs](clock.rs) | Restart-monotonic authority-clock ticks for outbox leases. |
| [handlers.rs](handlers.rs) | Production adapters for the closed E0-E3 and F0 destination inventory. |
| [pump.rs](pump.rs) | Owned bounded task that claims, routes, and settles durable outbox rows. |
| [qualification.rs](qualification.rs) | Bounded public-admin qualification for an effect-before-ack outbox restart. |
| [router.rs](router.rs) | Exact destination-to-handler routing for claimed outbox rows. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [claims/](claims/AGENTS.md) | Implementation pieces for `claims.rs`: Typed, effect-free decoding of claimed C0 outbox rows. |
| [qualification/](qualification/AGENTS.md) | Implementation pieces for `qualification.rs`: Bounded public-admin qualification for an effect-before-ack outbox restart. |

Owning module: [outbox.rs](../outbox.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
