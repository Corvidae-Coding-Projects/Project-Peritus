# crates/app/peritus-tui/src/model/chat/workbench/files — navigation

## Responsibility

Implementation pieces for `files.rs`: Deliberate file preview/confirmation and retained selection, never automatic path discovery.

## Files in this folder

| File | Role |
| --- | --- |
| [import.rs](import.rs) | Acknowledgement-driven external-text upload and immutable preview confirmation. |
| [keys.rs](keys.rs) | Keyboard actions are separate from inert draft editing and explicit confirmation. |

Owning module: [files.rs](../files.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
