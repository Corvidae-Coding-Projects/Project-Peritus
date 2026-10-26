# crates/app/peritus-launcher/src/update/install — navigation

## Responsibility

Implementation pieces for `install.rs`: Transactional native installer invocation and installed-version verification.

## Files in this folder

| File | Role |
| --- | --- |
| [windows_parent.rs](windows_parent.rs) | Binds deferred installation to the launcher's native process creation identity. |

Owning module: [install.rs](../install.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
