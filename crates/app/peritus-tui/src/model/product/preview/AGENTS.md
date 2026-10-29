# crates/app/peritus-tui/src/model/product/preview — navigation

## Responsibility

Implementation pieces for `preview.rs`: Explicit preview launch, interaction, capture, and evidence inspection.

## Files in this folder

| File | Role |
| --- | --- |
| [profile.rs](profile.rs) | Explicit selection of host-observed files; launch always rechecks these identities. |
| [terminal.rs](terminal.rs) | Explicit attachment to the latest running interactive preview. |

Owning module: [preview.rs](../preview.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
