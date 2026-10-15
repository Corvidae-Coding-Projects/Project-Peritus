# crates — navigation

## Responsibility

Rust implementation grouped by architectural responsibility. Choose a layer by the behavior you need, then follow its package and module maps.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [analysis/](analysis/AGENTS.md) | Trace diagnosis, reproducible evaluation, and evidence-based harness evolution/promotion. |
| [app/](app/AGENTS.md) | Product composition and human interfaces: launcher, CLI/TUI/WebUI, provider onboarding, durable product state, and daemon-owned coding runs. |
| [extensions/](extensions/AGENTS.md) | External integration boundaries: MCP bridge, plugin manifest/protocol SDK, and isolated plugin execution host. |
| [foundation/](foundation/AGENTS.md) | Shared identities, bounded codecs, authority/policy contracts, budgets, and acceptance/release vocabulary used by higher layers. |
| [model/](model/AGENTS.md) | Provider-neutral requests/events and transport adapters for account runtimes and HTTP providers. Execution authority stays with Peritus tools. |
| [observe/](observe/AGENTS.md) | Trace and telemetry projections/exporters; these describe behavior and retained evidence rather than authorizing effects. |
| [orchestration/](orchestration/AGENTS.md) | Agent/provider turns, role views, context/memory, scheduling, collaboration, review, gate planning, candidate settlement, and durable domain drivers. |
| [runtime/](runtime/AGENTS.md) | Concrete host effects: process ownership, sandboxes, network access, secret delivery, filesystem patches, workspaces, and Git operations. |
| [state/](state/AGENTS.md) | Durable persistence and its lifecycle: journal, artifacts, approvals, leases, evidence, migrations, and replayable projections. |
| [tools/](tools/AGENTS.md) | Authorized model-facing operations and dispatch contracts, including filesystem, shell, Git, quality operations, browser control, and desktop control. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
