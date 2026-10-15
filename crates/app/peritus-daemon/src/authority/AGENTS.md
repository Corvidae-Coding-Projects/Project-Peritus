# crates/app/peritus-daemon/src/authority — navigation

## Responsibility

Implementation pieces for `authority.rs`: Serialized authority actor and bounded client.

## Files in this folder

| File | Role |
| --- | --- |
| [message.rs](message.rs) | Closed messages accepted by the single writable-state owner. |
| [owner.rs](owner.rs) | Single writable-state authority owner and bounded client facade. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [owner/](owner/AGENTS.md) | Implementation pieces for `owner.rs`: Single writable-state authority owner and bounded client facade. |

Owning module: [authority.rs](../authority.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
