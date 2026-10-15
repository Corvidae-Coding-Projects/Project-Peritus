# crates/app/peritus-provider-onboarding — navigation

## Responsibility

User-facing provider discovery and official account login for Peritus.

Package `peritus-provider-onboarding` is a member of the root Cargo workspace; architectural owner `G4` in `app`.

Local dependency waypoints: [peritus-provider-opencode](../../model/peritus-provider-opencode/AGENTS.md), [peritus-provider-core](../../model/peritus-provider-core/AGENTS.md), [peritus-product-state](../peritus-product-state/AGENTS.md), [peritus-provider-anthropic](../../model/peritus-provider-anthropic/AGENTS.md), [peritus-provider-openai](../../model/peritus-provider-openai/AGENTS.md), [peritus-secrets](../../runtime/peritus-secrets/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-onboarding`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Provider discovery, selection, credential references, official runtime installation/login, model discovery, and real connection stages. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
