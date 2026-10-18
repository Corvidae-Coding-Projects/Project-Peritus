# Peritus tools guide

## Folder guide

Authorized model-facing operations and dispatch contracts, including filesystem, shell, Git, quality operations, browser control, and desktop control.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [peritus-tools-web/](peritus-tools-web/AGENTS.md) | Native web reads, immutable sources and keyless SearXNG search. |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-tool-protocol/](peritus-tool-protocol/AGENTS.md) | Bounded canonical model-facing tool protocol for Peritus. |
| [peritus-tool-router/](peritus-tool-router/AGENTS.md) | Exact-authority tool registry, router, dispatch, control, and replay boundary. |
| [peritus-tools-computer/](peritus-tools-computer/AGENTS.md) | Persistent browser and desktop automation host adapters for Peritus. |
| [peritus-tools-fs/](peritus-tools-fs/AGENTS.md) | Authorized bounded filesystem tools for Peritus workspaces. |
| [peritus-tools-git/](peritus-tools-git/AGENTS.md) | Authorized structured Git tools for Peritus workspaces. |
| [peritus-tools-quality/](peritus-tools-quality/AGENTS.md) | Explicit quality check discovery, execution, and evidence adapters for Peritus. |
| [peritus-tools-shell/](peritus-tools-shell/AGENTS.md) | Authorized structured argv and explicit script tools for Peritus. |

