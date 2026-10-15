# crates/app/peritus-tui/src/model/chat/workbench — navigation

## Responsibility

Implementation pieces for `workbench.rs`: Durable control intent tracking; reconnect resolves the original operation, never invents one.

## Files in this folder

| File | Role |
| --- | --- |
| [brief.rs](brief.rs) | User-confirmed field edits retain the draft until an exact durable receipt arrives. |
| [checkpoints.rs](checkpoints.rs) | Explicit bounded checkpoint creation and preview-bound rewind confirmation. |
| [compaction.rs](compaction.rs) | Explicit preview and confirmation for deterministic local source-handle reduction. |
| [context.rs](context.rs) | Read-only context commands preserve drafts and select exact revision-fenced views. |
| [conversation.rs](conversation.rs) | Composer admission through the same immutable queue and exact receipts as /queue. |
| [files.rs](files.rs) | Deliberate file preview/confirmation and retained selection, never automatic path discovery. |
| [fork.rs](fork.rs) | Checkpoint-bound conversation forks. |
| [goal.rs](goal.rs) | Explicit persistent-goal confirmation and revision-fenced pause, resume, and usage. |
| [images.rs](images.rs) | Explicit import drafts and preview-bound confirmation. No input is included before a receipt. |
| [init.rs](init.rs) | Read-only project initialization discovery and exact reviewed-diff submission. |
| [library.rs](library.rs) | Read-only session paging and exact keyboard selection; opening never starts execution. |
| [memory.rs](memory.rs) | Explicit project-guidance inspection and revision-fenced lifecycle commands. |
| [navigation.rs](navigation.rs) | Workbench refresh routing, panel keys, feature gates, and pending-request detection. |
| [permissions.rs](permissions.rs) | Effective permission inspection and explicit revision-fenced restriction changes. |
| [queue.rs](queue.rs) | Queue commands select exact inspected revisions, preserving the composer until acceptance. |
| [receipts.rs](receipts.rs) | Exact receipt correlation and explicit retry after an uncertain transport outcome. |
| [refresh.rs](refresh.rs) | Refresh metadata-bound commands before creating an operation from a stale panel. |
| [sessions.rs](sessions.rs) | Conversation-library navigation and metadata commands. |
| [state.rs](state.rs) | Small shared state enums for the workbench panel. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [brief/](brief/AGENTS.md) | Implementation pieces for `brief.rs`: User-confirmed field edits retain the draft until an exact durable receipt arrives. |
| [checkpoints/](checkpoints/AGENTS.md) | Implementation pieces for `checkpoints.rs`: Explicit bounded checkpoint creation and preview-bound rewind confirmation. |
| [conversation/](conversation/AGENTS.md) | Implementation pieces for `conversation.rs`: Composer admission through the same immutable queue and exact receipts as /queue. |
| [files/](files/AGENTS.md) | Implementation pieces for `files.rs`: Deliberate file preview/confirmation and retained selection, never automatic path discovery. |
| [goal/](goal/AGENTS.md) | Implementation pieces for `goal.rs`: Explicit persistent-goal confirmation and revision-fenced pause, resume, and usage. |
| [images/](images/AGENTS.md) | Implementation pieces for `images.rs`: Explicit import drafts and preview-bound confirmation. No input is included before a receipt. |
| [init/](init/AGENTS.md) | Implementation pieces for `init.rs`: Read-only project initialization discovery and exact reviewed-diff submission. |
| [navigation/](navigation/AGENTS.md) | Implementation pieces for `navigation.rs`: Workbench refresh routing, panel keys, feature gates, and pending-request detection. |
| [receipts/](receipts/AGENTS.md) | Implementation pieces for `receipts.rs`: Exact receipt correlation and explicit retry after an uncertain transport outcome. |

Owning module: [workbench.rs](../workbench.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
