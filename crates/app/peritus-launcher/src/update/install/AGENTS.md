# crates/app/peritus-launcher/src/update/install — navigation

## Responsibility

Implementation pieces for `install.rs`: Transactional native installer invocation and installed-version verification.

## Files in this folder

| File | Role |
| --- | --- |
| [windows_parent.rs](windows_parent.rs) | Binds deferred installation to the launcher's native process creation identity. |

Owning module: [install.rs](../install.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
