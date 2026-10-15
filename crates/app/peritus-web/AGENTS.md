# crates/app/peritus-web — navigation

## Responsibility

Local browser workspace and gateway for the Peritus harness.

Package `peritus-web` is a member of the root Cargo workspace; architectural owner `G1` in `app`.

Local dependency waypoints: [peritus-app-client](../peritus-app-client/AGENTS.md), [peritus-app-protocol](../peritus-app-protocol/AGENTS.md), [peritus-process](../../runtime/peritus-process/AGENTS.md), [peritus-product-runner](../peritus-product-runner/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md), [peritus-local-socket](../../runtime/peritus-local-socket/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-web`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Loopback HTTP gateway, browser workspace/session service, CLI PTY console bridge, static assets, and daemon-facing routes. |

## Nearby owners

- [Browser client](../../../webui/AGENTS.md).
- [Daemon client transport](../peritus-app-client/AGENTS.md).
- [Native setup path bridged by the console](../peritus-launcher/AGENTS.md).

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
