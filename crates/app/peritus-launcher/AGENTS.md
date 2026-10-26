# crates/app/peritus-launcher — navigation

## Responsibility

Single-command local bootstrap and product composition for Peritus.

Package `peritus-launcher` is a member of the root Cargo workspace; architectural owner `G4` in `app`.

Local dependency waypoints: [peritus-approval](../../state/peritus-approval/AGENTS.md), [peritus-app-protocol](../peritus-app-protocol/AGENTS.md), [peritus-daemon](../peritus-daemon/AGENTS.md), [peritus-git](../../runtime/peritus-git/AGENTS.md), [peritus-local-socket](../../runtime/peritus-local-socket/AGENTS.md), [peritus-model-protocol](../../model/peritus-model-protocol/AGENTS.md), [peritus-provider-core](../../model/peritus-provider-core/AGENTS.md), [peritus-provider-onboarding](../peritus-provider-onboarding/AGENTS.md), [peritus-product-state](../peritus-product-state/AGENTS.md), [peritus-tui](../peritus-tui/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-launcher`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Single-command bootstrap: product state, workspace trust/selection, provider setup, packaged daemon resolution/startup, update handling, and UI handoff. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
