# crates/app/peritus-daemon/src/config — navigation

## Responsibility

Implementation pieces for `config.rs`: Strict version-one daemon configuration.

## Files in this folder

| File | Role |
| --- | --- |
| [approval.rs](approval.rs) | Public approval credential-registry configuration. |
| [catalog.rs](catalog.rs) | Strict project, workspace, and tool inventory declarations. |
| [context.rs](context.rs) | Local working memory and explicit offline provider-route admission. |
| [deserialize.rs](deserialize.rs) | Exact Serde adapters for the strict external configuration representation. |
| [folder.rs](folder.rs) | Explicit direct-folder declarations, distinct from Git-backed C1 registrations. |
| [paths.rs](paths.rs) | Protected daemon filesystem-root configuration. |
| [product.rs](product.rs) | Product-run behavior selected explicitly by the local user. |
| [provider.rs](provider.rs) | Strict immutable C5 provider route declarations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [provider/](provider/AGENTS.md) | Implementation pieces for `provider.rs`: Strict immutable C5 provider route declarations. |

Owning module: [config.rs](../config.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
