# peritus-mcp

A bounded MCP JSON-RPC stdio server that projects Peritus tools, resources, and prompts through a
daemon-provided bridge. It implements initialization, pagination, tool calls, resource reads,
prompt rendering, ping, cancellation, concurrency bounds, and truthful JSON-RPC failures. The MCP
layer cannot create a C4 invocation permit or B1 capability; the bridge implementation must route
each request through current G0/A3/C4 authority.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-mcp
```
