# crates/state/peritus-evidence — navigation

## Responsibility

Durable provenance, freshness, invalidation, and portable evidence bundles for Peritus.

Package `peritus-evidence` is a member of the root Cargo workspace; architectural owner `C0` in `state`.

Local dependency waypoints: [peritus-artifact-store](../peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-journal](../peritus-journal/AGENTS.md), [peritus-projection](../peritus-projection/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-quality-policy](../../foundation/peritus-quality-policy/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-evidence`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable evidence provenance and portable bundle assembly for Peritus. |

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
