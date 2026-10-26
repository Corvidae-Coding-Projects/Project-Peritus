# crates/foundation/peritus-kernel — navigation

## Responsibility

lifecycle state machines and authoritative event reduction for Peritus.

Package `peritus-kernel` is a member of the root Cargo workspace; architectural owner `B0` in `foundation`.

Local dependency waypoints: [peritus-budget](../peritus-budget/AGENTS.md), [peritus-policy](../peritus-policy/AGENTS.md), [peritus-quality-policy](../peritus-quality-policy/AGENTS.md), [peritus-spec](../peritus-spec/AGENTS.md), [peritus-types](../peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-kernel`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Lifecycle state machines and authoritative event reduction for Peritus. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
