# crates/runtime/peritus-plugin-host — navigation

## Responsibility

Isolated process and Wasm plugin discovery, trust, lifecycle, and quota host.

Package `peritus-plugin-host` is a member of the root Cargo workspace; architectural owner `G3` in `runtime`.

Local dependency waypoints: [peritus-plugin-sdk](../../foundation/peritus-plugin-sdk/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-plugin-host`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Host for untrusted out-of-process and Wasm Peritus plugins. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
