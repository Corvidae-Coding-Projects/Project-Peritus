# crates/app/peritus-web/src/files — navigation

## Responsibility

Implementation pieces for `files.rs`: Bounded directory pages and canonical project-confined file access.

## Files in this folder

| File | Role |
| --- | --- |
| [attachments.rs](attachments.rs) | Immutable local selections. Viewing never attaches; sending names exact retained snapshots. |
| [edit.rs](edit.rs) | Optimistic, atomic text saves. The journal holds hashes, never whole file bodies. |
| [inventory.rs](inventory.rs) | Stable directory pages; filesystem scans occur only when a new inventory is requested. |
| [pdf.rs](pdf.rs) | Bounded PDF identification before native browser delivery, never HTML sniffing. |
| [range.rs](range.rs) | UTF-8 byte-range previews without whole-file reads or hashing. |

Owning module: [files.rs](../files.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
