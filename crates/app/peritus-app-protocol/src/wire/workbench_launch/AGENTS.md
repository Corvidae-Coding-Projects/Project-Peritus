# crates/app/peritus-app-protocol/src/wire/workbench_launch — navigation

## Responsibility

Implementation pieces for `workbench_launch.rs`: Canonical bounded launch, capture, result and artifact-feedback codecs.

## Files in this folder

| File | Role |
| --- | --- |
| [output.rs](output.rs) | Preview output codec. |
| [profile.rs](profile.rs) | Canonical launch-profile and launch-source codecs. |

Owning module: [workbench_launch.rs](../workbench_launch.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
