# crates/tools — navigation

## Responsibility

Authorized model-facing operations and dispatch contracts, including filesystem, shell, Git, quality operations, browser control, and desktop control.

## Files in this folder

| File | Role |
| --- | --- |
| [peritus-tools-web/](peritus-tools-web/AGENTS.md) | Native web reads, immutable sources and built-in keyless Rust search. |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-tool-protocol/](peritus-tool-protocol/AGENTS.md) | Bounded canonical model-facing tool protocol for Peritus. |
| [peritus-tool-router/](peritus-tool-router/AGENTS.md) | Exact-authority tool registry, router, dispatch, control, and replay boundary. |
| [peritus-tools-computer/](peritus-tools-computer/AGENTS.md) | Persistent browser and desktop automation host adapters for Peritus. |
| [peritus-tools-fs/](peritus-tools-fs/AGENTS.md) | Authorized bounded filesystem tools for Peritus workspaces. |
| [peritus-tools-git/](peritus-tools-git/AGENTS.md) | Authorized structured Git tools for Peritus workspaces. |
| [peritus-tools-quality/](peritus-tools-quality/AGENTS.md) | Explicit quality check discovery, execution, and evidence adapters for Peritus. |
| [peritus-tools-shell/](peritus-tools-shell/AGENTS.md) | Authorized structured argv and explicit script tools for Peritus. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
