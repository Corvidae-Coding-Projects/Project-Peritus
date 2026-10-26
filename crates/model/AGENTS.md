# crates/model — navigation

## Responsibility

Provider-neutral requests/events and transport adapters for account runtimes and HTTP providers. Execution authority stays with Peritus tools.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-model-protocol/](peritus-model-protocol/AGENTS.md) | Provider-neutral bounded model request and normalized stream protocol for Peritus. |
| [peritus-provider-anthropic/](peritus-provider-anthropic/AGENTS.md) | First-party Anthropic Messages adapter for Peritus. |
| [peritus-provider-compatible/](peritus-provider-compatible/AGENTS.md) | Explicitly profiled compatible model endpoints for Peritus. |
| [peritus-provider-core/](peritus-provider-core/AGENTS.md) | Owned HTTP transport, cancellation, retry, and model-provider interfaces for Peritus. |
| [peritus-provider-google/](peritus-provider-google/AGENTS.md) | First-party Google Gemini Interactions and Generate Content adapter for Peritus. |
| [peritus-provider-openai/](peritus-provider-openai/AGENTS.md) | First-party OpenAI Responses adapter for Peritus. |
| [peritus-provider-opencode/](peritus-provider-opencode/AGENTS.md) | Official OpenCode model transport for Peritus. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
