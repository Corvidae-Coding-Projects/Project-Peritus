# crates/extensions — navigation

## Responsibility

External integration boundaries: MCP bridge, plugin manifest/protocol SDK, and isolated plugin execution host.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-mcp/](peritus-mcp/AGENTS.md) | Bounded MCP JSON-RPC server and non-authoritative Peritus bridge. |
| [peritus-plugin-host/](peritus-plugin-host/AGENTS.md) | Isolated process and Wasm plugin discovery, trust, lifecycle, and quota host. |
| [peritus-plugin-sdk/](peritus-plugin-sdk/AGENTS.md) | Versioned wire contracts and canonical manifests for isolated Peritus plugins. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
