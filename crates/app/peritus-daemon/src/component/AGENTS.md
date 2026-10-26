# crates/app/peritus-daemon/src/component — navigation

## Responsibility

Explicit application component inventories.

## Files in this folder

| File | Role |
| --- | --- |
| [connection.rs](connection.rs) | User-triggered connection tests through the production registry and credential broker. |
| [credentials.rs](credentials.rs) | C3-backed lazy credential resolution for direct C5 adapters. |
| [hosted.rs](hosted.rs) | Named-service composition through the same native adapters used by ordinary daemon routes. |
| [inventory.rs](inventory.rs) | Immutable startup inventory for configured providers and tools. |
| [mod.rs](mod.rs) | Explicit application component inventories. |
| [profiles.rs](profiles.rs) | Typed provider declarations accepted by the daemon composition root. |
| [providers.rs](providers.rs) | Bounded immutable construction of configured C5 provider adapters. |
| [tools.rs](tools.rs) | Closed production C4 tool inventory and router composition. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [tools/](tools/AGENTS.md) | Implementation pieces for `tools.rs`: Closed production C4 tool inventory and router composition. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
