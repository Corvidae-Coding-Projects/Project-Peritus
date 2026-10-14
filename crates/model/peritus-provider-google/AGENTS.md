# crates/model/peritus-provider-google — navigation

## Responsibility

First-party Google Gemini Interactions and Generate Content adapter for Peritus.

Package `peritus-provider-google` is a member of the root Cargo workspace; architectural owner `C5` in `model`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-model-protocol](../peritus-model-protocol/AGENTS.md), [peritus-provider-core](../peritus-provider-core/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-google`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [fixtures/](fixtures/AGENTS.md) | Retained wire/provider fixture inputs for `peritus-provider-google`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code. |
| [src/](src/AGENTS.md) | First-party stable-v1 Google Gemini adapter for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
