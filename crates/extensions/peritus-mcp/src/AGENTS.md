# crates/extensions/peritus-mcp/src — navigation

## Responsibility

Bounded MCP server and non-authoritative bridge for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [bridge.rs](bridge.rs) | Non-authoritative G0/A3/C4 bridge contract and projections. |
| [cancellation.rs](cancellation.rs) | Cooperative cancellation for an active MCP bridge request. |
| [error.rs](error.rs) | MCP transport and authority-bridge failures. |
| [framing.rs](framing.rs) | Bounded newline-delimited MCP stdio framing. |
| [jsonrpc.rs](jsonrpc.rs) | Strict JSON-RPC 2.0 request and response envelopes. |
| [lib.rs](lib.rs) | Bounded MCP server and non-authoritative bridge for Peritus. |
| [protocol.rs](protocol.rs) | MCP wire DTOs and method parameter validation. |
| [server.rs](server.rs) | Concurrent bounded MCP JSON-RPC server lifecycle and dispatch. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bridge/](bridge/AGENTS.md) | Implementation pieces for `bridge.rs`: Non-authoritative G0/A3/C4 bridge contract and projections. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
