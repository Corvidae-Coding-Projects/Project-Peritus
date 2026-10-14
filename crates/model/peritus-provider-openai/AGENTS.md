# crates/model/peritus-provider-openai — navigation

## Responsibility

First-party OpenAI Responses adapter for Peritus.

Package `peritus-provider-openai` is a member of the root Cargo workspace; architectural owner `C5` in `model`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-model-protocol](../peritus-model-protocol/AGENTS.md), [peritus-provider-core](../peritus-provider-core/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-openai`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [fixtures/](fixtures/AGENTS.md) | Retained wire/provider fixture inputs for `peritus-provider-openai`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code. |
| [src/](src/AGENTS.md) | OpenAI HTTP Responses adapter and official Codex account runtime transport, with separate request/stream/runtime paths. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
