# crates/runtime/peritus-workspace — navigation

## Responsibility

Target-owned workspace authorization, snapshots, rollback, and restart reconciliation.

Package `peritus-workspace` is a member of the root Cargo workspace; architectural owner `C1` in `runtime`.

Candidate checkpoint, nested-repository export, and discard live in [task_candidate](src/task_candidate/AGENTS.md); workspace creation remains in the original candidate module.

Local dependency waypoints: [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-git](../peritus-git/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-kernel](../../foundation/peritus-kernel/AGENTS.md), [peritus-leases](../../state/peritus-leases/AGENTS.md), [peritus-patch](../peritus-patch/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-workspace`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Target-owned authorization and lifecycle boundary for isolated Peritus workspaces. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
