# crates/app/peritus-launcher/src/bootstrap/configuration — navigation

## Responsibility

Implementation pieces for `configuration.rs`: Strict immutable daemon configuration generated from durable product state.

## Files in this folder

| File | Role |
| --- | --- |
| [account.rs](account.rs) | Reconcile official-client updates before immutable configuration publication. |
| [compatibility.rs](compatibility.rs) | Narrow byte compatibility for immutable configuration generations from prior releases. |
| [folder.rs](folder.rs) | Direct-folder configuration, independent of canonical Git registrations. |
| [folder_catalog.rs](folder_catalog.rs) | Reconcile only durable direct-folder declarations into a new immutable generation. |
| [hosted.rs](hosted.rs) | Named hosted provider route rendering and durable OpenCode transport selection migration; shared format compatibility lives in compatibility.rs. |
| [native.rs](native.rs) | Conservative discovery of complete packaged Linux isolation components for generated daemon configuration. |

Owning module: [configuration.rs](../configuration.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
