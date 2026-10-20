# crates/app/peritus-tui — navigation

## Responsibility

Interactive terminal client for the Peritus daemon.

Package `peritus-tui` is a member of the root Cargo workspace; architectural owner `G2` in `app`.

Local dependency waypoints: [peritus-app-protocol](../peritus-app-protocol/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-product-state](../peritus-product-state/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md), [peritus-run-settlement](../../orchestration/peritus-run-settlement/AGENTS.md), [peritus-tools-shell](../../tools/peritus-tools-shell/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tui`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Native terminal application state, daemon event handling, interaction routing, rendering, connection lifecycle, and shared text editing. |

## Nearby owners

- [Shared daemon client](../peritus-app-client/AGENTS.md).
- [Typed interaction protocol](../peritus-app-protocol/AGENTS.md).
- [Durable authority behind UI controls](../peritus-daemon/src/product_run/AGENTS.md).

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
