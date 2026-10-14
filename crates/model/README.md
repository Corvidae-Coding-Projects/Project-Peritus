# Peritus model guide

## Folder guide

Provider-neutral requests/events and transport adapters for account runtimes and HTTP providers. Execution authority stays with Peritus tools.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-model-protocol/](peritus-model-protocol/AGENTS.md) | Provider-neutral bounded model request and normalized stream protocol for Peritus. |
| [peritus-provider-anthropic/](peritus-provider-anthropic/AGENTS.md) | First-party Anthropic Messages adapter for Peritus. |
| [peritus-provider-compatible/](peritus-provider-compatible/AGENTS.md) | Explicitly profiled compatible model endpoints for Peritus. |
| [peritus-provider-core/](peritus-provider-core/AGENTS.md) | Owned HTTP transport, cancellation, retry, and model-provider interfaces for Peritus. |
| [peritus-provider-google/](peritus-provider-google/AGENTS.md) | First-party Google Gemini Interactions and Generate Content adapter for Peritus. |
| [peritus-provider-openai/](peritus-provider-openai/AGENTS.md) | First-party OpenAI Responses adapter for Peritus. |
| [peritus-provider-opencode/](peritus-provider-opencode/AGENTS.md) | Official OpenCode model transport for Peritus. |

