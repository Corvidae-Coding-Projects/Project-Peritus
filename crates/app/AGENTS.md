# crates/app — navigation

## Responsibility

Product composition and human interfaces: launcher, CLI/TUI/WebUI, provider onboarding, durable product state, and daemon-owned coding runs.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

## Child areas

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

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
