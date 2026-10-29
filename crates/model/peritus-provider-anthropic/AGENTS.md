# crates/model/peritus-provider-anthropic — navigation

## Responsibility

First-party Anthropic Messages adapter for Peritus.

Package `peritus-provider-anthropic` is a member of the root Cargo workspace; architectural owner `C5` in `model`.

Local dependency waypoints: [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-model-protocol](../peritus-model-protocol/AGENTS.md), [peritus-provider-core](../peritus-provider-core/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-anthropic`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [fixtures/](fixtures/AGENTS.md) | Retained wire/provider fixture inputs for `peritus-provider-anthropic`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code. |
| [src/](src/AGENTS.md) | Anthropic Messages HTTP adapter and official Claude account runtime transport, with separate native and HTTP encoding/decoding. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
