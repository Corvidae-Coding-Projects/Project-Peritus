# crates/app/peritus-tui/src/render/chat — navigation

## Responsibility

Implementation pieces for `chat.rs`: Conversation-first terminal layout with a persistent multiline composer and scrollable text.

## Files in this folder

| File | Role |
| --- | --- |
| [brief.rs](brief.rs) | Exact confirmed fields, source revisions and eligibility; observed execution is separate. |
| [checkpoints.rs](checkpoints.rs) | Exact covered-path checkpoint and rewind preview without workspace mutation on inspection. |
| [compaction.rs](compaction.rs) | Content-free local compaction preview; exact reply text remains in immutable archives. |
| [context.rs](context.rs) | Content-free context inspector. The composer retains a separate non-overlapping area. |
| [doctor.rs](doctor.rs) | Responsive read-only diagnostic panel with independent scrolling and visible return action. |
| [effort.rs](effort.rs) | Reasoning effort remains discoverable even when model discovery is unavailable. |
| [files.rs](files.rs) | Exact file/range consent and current selection, with no implied provider delivery. |
| [goal.rs](goal.rs) | Truthful goal, criterion, and usage projection. |
| [images.rs](images.rs) | Readable exact-byte consent preview; pixels are not required for a terminal confirmation. |
| [init.rs](init.rs) | Exact initialization source, command, and file-diff review panel. |
| [inspector.rs](inspector.rs) | Shared wrapping and bounds for the metadata inspectors; rendering and keys use the same rows. |
| [library.rs](library.rs) | Session identities remain selectable and visible independently of the metadata inspector. |
| [memory.rs](memory.rs) | Exact project-guidance projection with content-free tombstone disclosure. |
| [permissions.rs](permissions.rs) | Effective host-intersected workspace permission projection. |
| [queue.rs](queue.rs) | Paginated input history, with exact row revisions and visibly separate lifecycle labels. |
| [status.rs](status.rs) | Connection-aware composer status stays visible even when provider labels fill the title. |
| [transcript.rs](transcript.rs) | Public activity projection and wrapping shared by rendering and mouse copy. |
| [workbench.rs](workbench.rs) | Bounded metadata inspector independent of composer and transcript scroll. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [images/](images/AGENTS.md) | Implementation pieces for `images.rs`: Readable exact-byte consent preview; pixels are not required for a terminal confirmation. |

Owning module: [chat.rs](../chat.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
