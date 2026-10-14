# peritus-mcp

## Folder guide

Bounded MCP JSON-RPC server and non-authoritative Peritus bridge.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-mcp`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Bounded MCP server and non-authoritative bridge for Peritus. |

A bounded MCP JSON-RPC stdio server that projects Peritus tools, resources, and prompts through a
daemon-provided bridge. It implements initialization, pagination, tool calls, resource reads,
prompt rendering, ping, cancellation, concurrency bounds, and truthful JSON-RPC failures. The MCP
layer cannot create a C4 invocation permit or B1 capability; the bridge implementation must route
each request through current G0/A3/C4 authority.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
