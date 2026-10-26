# crates/extensions/peritus-mcp — navigation

## Responsibility

Bounded MCP JSON-RPC server and non-authoritative Peritus bridge.

Package `peritus-mcp` is a member of the root Cargo workspace; architectural owner `G3` in `extensions`.

Local dependency waypoints: [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-tool-protocol](../../tools/peritus-tool-protocol/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-mcp`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Bounded MCP server and non-authoritative bridge for Peritus. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
