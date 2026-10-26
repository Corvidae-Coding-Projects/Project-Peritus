# crates/foundation/peritus-protocol — navigation

## Responsibility

Stable canonical domain protocol for Peritus.

Package `peritus-protocol` is a member of the root Cargo workspace; architectural owner `B3` in `foundation`.

Local dependency waypoints: [peritus-budget](../peritus-budget/AGENTS.md), [peritus-codec](../peritus-codec/AGENTS.md), [peritus-kernel](../peritus-kernel/AGENTS.md), [peritus-policy](../peritus-policy/AGENTS.md), [peritus-quality-policy](../peritus-quality-policy/AGENTS.md), [peritus-spec](../peritus-spec/AGENTS.md), [peritus-types](../peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-protocol`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Stable canonical domain protocol for Peritus. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
