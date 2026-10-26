# crates/tools/peritus-tools-quality — navigation

## Responsibility

Explicit quality check discovery, execution, and evidence adapters for Peritus.

Package `peritus-tools-quality` is a member of the root Cargo workspace; architectural owner `C4` in `tools`.

Local dependency waypoints: [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-patch](../../runtime/peritus-patch/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-process](../../runtime/peritus-process/AGENTS.md), [peritus-quality-policy](../../foundation/peritus-quality-policy/AGENTS.md), [peritus-sandbox](../../runtime/peritus-sandbox/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-tool-protocol](../peritus-tool-protocol/AGENTS.md), [peritus-tool-router](../peritus-tool-router/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-quality`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Explicit quality-check discovery, invocation, and candidate B2 evidence. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
