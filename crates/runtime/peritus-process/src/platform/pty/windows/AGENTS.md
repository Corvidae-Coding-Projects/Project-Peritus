# crates/runtime/peritus-process/src/platform/pty/windows — navigation

## Responsibility

Implementation pieces for `windows.rs`: Native `ConPTY` launch with suspended-process Job Object admission.

## Files in this folder

| File | Role |
| --- | --- |
| [launch.rs](launch.rs) | Literal command/environment projection and suspended native process creation. |
| [native.rs](native.rs) | Uniquely owned kernel handles, job accounting, and pseudoconsole lifetime. |

Owning module: [windows.rs](../windows.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
