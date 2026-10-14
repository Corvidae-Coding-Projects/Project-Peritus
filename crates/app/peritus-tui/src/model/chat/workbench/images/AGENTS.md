# crates/app/peritus-tui/src/model/chat/workbench/images — navigation

## Responsibility

Implementation pieces for `images.rs`: Explicit import drafts and preview-bound confirmation. No input is included before a receipt.

## Files in this folder

| File | Role |
| --- | --- |
| [editor.rs](editor.rs) | Local path/caption editing is inert; only separate keyboard actions read or confirm. |
| [page.rs](page.rs) | Revision-fenced retained-image inspection and explicit future-selection changes. |
| [upload.rs](upload.rs) | Acknowledgement-driven upload; only completion requests the daemon's decoded preview. |

Owning module: [images.rs](../images.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
