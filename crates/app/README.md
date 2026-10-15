# Peritus app guide

## Folder guide

Product composition and human interfaces: launcher, CLI/TUI/WebUI, provider onboarding, durable product state, and daemon-owned coding runs.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-app-client/](peritus-app-client/AGENTS.md) | Shared local application-protocol client for Peritus interfaces. |
| [peritus-app-protocol/](peritus-app-protocol/AGENTS.md) | Transport-neutral application protocol contract for Peritus clients and daemons. |
| [peritus-cli/](peritus-cli/AGENTS.md) | Scriptable command-line client for the Peritus daemon. |
| [peritus-daemon/](peritus-daemon/AGENTS.md) | Production daemon and application composition owner for Peritus. |
| [peritus-launcher/](peritus-launcher/AGENTS.md) | Single-command local bootstrap and product composition for Peritus. |
| [peritus-product-runner/](peritus-product-runner/AGENTS.md) | Daemon-owned writer-reviewer-fixer product run coordinator for Peritus. |
| [peritus-product-state/](peritus-product-state/AGENTS.md) | Pure resumable product-state model for the Peritus application experience. |
| [peritus-provider-onboarding/](peritus-provider-onboarding/AGENTS.md) | User-facing provider discovery and official account login for Peritus. |
| [peritus-tui/](peritus-tui/AGENTS.md) | Interactive terminal client for the Peritus daemon. |
| [peritus-web/](peritus-web/AGENTS.md) | Local browser workspace and gateway for the Peritus harness. |
| [testing/](testing/AGENTS.md) | Retained grouping with active package/release/external-benchmark components and README-only records of retired test components. Cargo workspace membership distinguishes them. |

