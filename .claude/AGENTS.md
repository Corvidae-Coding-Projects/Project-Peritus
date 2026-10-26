# .claude — navigation

## Responsibility

Local Claude integration configuration, hooks, and MCP utility code.

## Files in this folder

| File | Role |
| --- | --- |
| [settings.json](settings.json) | Structured settings data with fields `enableAllProjectMcpServers`, `hooks`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [hooks/](hooks/AGENTS.md) | Claude event-hook implementations and their local configuration helper. |
| [mcp/](mcp/AGENTS.md) | Local MCP utility service code used by the Claude integration. |

## Working in this area

Use the responsibility and tables above to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
