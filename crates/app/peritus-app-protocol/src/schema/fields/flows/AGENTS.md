# crates/app/peritus-app-protocol/src/schema/fields/flows — navigation

## Responsibility

Implementation pieces for `flows.rs`: Stateful application-protocol metadata grouped by stable concern.

## Files in this folder

| File | Role |
| --- | --- |
| [brief.rs](brief.rs) | Exact brief wire fields and bounded user-input provenance. |
| [brief_pages.rs](brief_pages.rs) | Additive exact brief inventory and immutable UTF-8 body paging. |
| [checkpoints.rs](checkpoints.rs) | Covered-path checkpoints, exact rewind previews, and durable restore receipts. |
| [compaction.rs](compaction.rs) | Deterministic local source-handle preview fields in canonical wire order. |
| [context.rs](context.rs) | Context metadata in exact canonical field order. Sizes never imply token accounting. |
| [daemon.rs](daemon.rs) | Daemon, heartbeat, and shutdown flow field metadata. |
| [doctor.rs](doctor.rs) | Bounded doctor wire and projection metadata. |
| [files.rs](files.rs) | File preview, range and retained-version wire descriptors. |
| [goal.rs](goal.rs) | Bounded persistent-goal definitions, controls, evidence, and cumulative accounting. |
| [image_page.rs](image_page.rs) | Exact retained-image metadata page descriptors. |
| [images.rs](images.rs) | Explicit-image fields in exact binary order, including mandatory preview model effort. |
| [init.rs](init.rs) | Exact approval-first project-initialization fields. |
| [init_artifacts.rs](init_artifacts.rs) | Additive immutable initialization artifact contracts. |
| [inputs.rs](inputs.rs) | Queue DTO metadata in exact canonical field order. |
| [interaction.rs](interaction.rs) | Conversation and model discovery metadata in canonical wire order. |
| [launch.rs](launch.rs) | Preview launch, selected-window capture, and independent result-evidence descriptors. |
| [library.rs](library.rs) | Conversation-library, public source snippet, and fork-lineage metadata. |
| [memory.rs](memory.rs) | Project-guidance DTO metadata in exact canonical field order. |
| [permissions.rs](permissions.rs) | Effective workspace permission and restriction metadata. |
| [product.rs](product.rs) | Product-run snapshot metadata. |
| [prompt_terminal.rs](prompt_terminal.rs) | Prompt-answer and terminal flow field metadata. |
| [review.rs](review.rs) | Structured-review mutation descriptors and their content-bound anchors. |
| [workbench.rs](workbench.rs) | Revisioned control projections in exact canonical field order. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [checkpoints/](checkpoints/AGENTS.md) | Implementation pieces for `checkpoints.rs`: Covered-path checkpoints, exact rewind previews, and durable restore receipts. |
| [launch/](launch/AGENTS.md) | Implementation pieces for `launch.rs`: Preview launch, selected-window capture, and independent result-evidence descriptors. |
| [product/](product/AGENTS.md) | Implementation pieces for `product.rs`: Product-run snapshot metadata. |
| [prompt_terminal/](prompt_terminal/AGENTS.md) | Implementation pieces for `prompt_terminal.rs`: Prompt-answer and terminal flow field metadata. |

Owning module: [flows.rs](../flows.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
