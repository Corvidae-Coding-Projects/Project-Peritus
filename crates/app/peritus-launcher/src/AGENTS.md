# crates/app/peritus-launcher/src — navigation

## Responsibility

Single-command bootstrap: product state, workspace trust/selection, provider setup, packaged daemon resolution/startup, update handling, and UI handoff.

## Files in this folder

| File | Role |
| --- | --- |
| [app.rs](app.rs) | End-to-end interactive product launch composition. |
| [bootstrap.rs](bootstrap.rs) | Resumable protected local bootstrap. |
| [daemon.rs](daemon.rs) | Packaged sibling-daemon resolution, startup, and bounded readiness. |
| [error.rs](error.rs) | Actionable launcher failures. |
| [identity.rs](identity.rs) | Operating-system random installation identity generation. |
| [layout.rs](layout.rs) | Platform-native application directory discovery. |
| [lib.rs](lib.rs) | Host composition for the single-command Peritus product experience. |
| [persistence.rs](persistence.rs) | Immutable-generation product state and atomic local publication. |
| [provider_setup.rs](provider_setup.rs) | First-run provider selection and focused repeat-launch repair. |
| [terminal.rs](terminal.rs) | Small line-oriented terminal boundary shared by product setup screens. |
| [update.rs](update.rs) | Cached public release discovery and native self-update composition. |
| [workspace_setup.rs](workspace_setup.rs) | Ergonomic repository selection, explicit trust, recent choices, and repair. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [daemon/](daemon/AGENTS.md) | Shared-daemon launch serialization and live workspace catalog publication. |
| [app/](app/AGENTS.md) | Implementation pieces for `app.rs`: End-to-end interactive product launch composition. |
| [bootstrap/](bootstrap/AGENTS.md) | Implementation pieces for `bootstrap.rs`: Resumable protected local bootstrap. |
| [provider_setup/](provider_setup/AGENTS.md) | Implementation pieces for `provider_setup.rs`: First-run provider selection and focused repeat-launch repair. |
| [update/](update/AGENTS.md) | Implementation pieces for `update.rs`: Cached public release discovery and native self-update composition. |
| [workspace_setup/](workspace_setup/AGENTS.md) | Implementation pieces for `workspace_setup.rs`: Ergonomic repository selection, explicit trust, recent choices, and repair. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
