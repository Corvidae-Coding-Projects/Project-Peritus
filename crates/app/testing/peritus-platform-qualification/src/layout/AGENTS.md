# crates/app/testing/peritus-platform-qualification/src/layout — navigation

## Responsibility

Implementation pieces for `layout.rs`: Concrete cross-target release layouts and ownership contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [contracts.rs](contracts.rs) | Ownership, entry-kind, and permission contracts for release-layout paths. |
| [path.rs](path.rs) | Target-independent validation for target-native absolute install paths. |

Owning module: [layout.rs](../layout.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
