# crates/orchestration/peritus-harness — navigation

## Responsibility

Durable checked harness definitions and exact workspace materialization.

Package `peritus-harness` is a member of the root Cargo workspace; architectural owner `E1` in `orchestration`.

Local dependency waypoints: [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-git](../../runtime/peritus-git/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-patch](../../runtime/peritus-patch/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-harness`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable checked harness definitions and exact C1 materialization. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
