# Peritus extensions guide

## Folder guide

External integration boundaries: MCP bridge, plugin manifest/protocol SDK, and isolated plugin execution host.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-mcp/](peritus-mcp/AGENTS.md) | Bounded MCP JSON-RPC server and non-authoritative Peritus bridge. |
| [peritus-plugin-host/](peritus-plugin-host/AGENTS.md) | Isolated process and Wasm plugin discovery, trust, lifecycle, and quota host. |
| [peritus-plugin-sdk/](peritus-plugin-sdk/AGENTS.md) | Versioned wire contracts and canonical manifests for isolated Peritus plugins. |

