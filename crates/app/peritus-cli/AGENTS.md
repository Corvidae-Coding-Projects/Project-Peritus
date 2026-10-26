# crates/app/peritus-cli — navigation

## Responsibility

Scriptable command-line client for the Peritus daemon.

Package `peritus-cli` is a member of the root Cargo workspace; architectural owner `G1` in `app`.

Local dependency waypoints: [peritus-trace](../../observe/peritus-trace/AGENTS.md), [peritus-tools-computer](../../tools/peritus-tools-computer/AGENTS.md), [peritus-app-client](../peritus-app-client/AGENTS.md), [peritus-app-protocol](../peritus-app-protocol/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-launcher](../peritus-launcher/AGENTS.md), [peritus-product-state](../peritus-product-state/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md), [peritus-run-settlement](../../orchestration/peritus-run-settlement/AGENTS.md), [peritus-tools-shell](../../tools/peritus-tools-shell/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-cli`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Scriptable daemon client plus interactive launch dispatch: parse user arguments, route typed requests, render responses, and manage terminal/artifact/event streams. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
