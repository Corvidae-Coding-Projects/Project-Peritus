# crates/app/peritus-app-protocol/src/workbench — navigation

## Responsibility

Implementation pieces for `workbench.rs`: Additive revisioned workbench controls. DTO validation never authenticates an actor.

## Files in this folder

| File | Role |
| --- | --- |
| [brief.rs](brief.rs) | User-confirmed brief projection with exact immutable input provenance. |
| [checkpoints.rs](checkpoints.rs) | Public bounded checkpoint coverage, rewind previews, and durable restore receipts. |
| [compaction.rs](compaction.rs) | Content-free deterministic prompt-view previews over immutable public replies. |
| [context.rs](context.rs) | Read-only bounded context metadata; never carries private model content or credentials. |
| [conversation.rs](conversation.rs) | Authenticated execution discovery for a selected durable conversation. |
| [execution.rs](execution.rs) | Explicit execution settings over an already accepted queue, not an embedded prompt macro. |
| [files.rs](files.rs) | Explicit workspace-file preview and selection. DTOs never confer read authority. |
| [goal.rs](goal.rs) | Persistent-goal, safe-boundary, evidence, and usage DTOs. |
| [images.rs](images.rs) | Explicit image upload and preview contracts; none imply provider delivery or path authority. |
| [init.rs](init.rs) | Bounded, approval-first project initialization contracts. |
| [inputs.rs](inputs.rs) | Bounded queue commands and revision-fenced paginated projections. |
| [intent.rs](intent.rs) | Closed typed workbench mutations; no variant itself authenticates or starts work. |
| [launch.rs](launch.rs) | Typed launch profiles, scoped capture receipts, and independently classified result evidence. |
| [library.rs](library.rs) | Bounded conversation-library search and non-running fork contracts. |
| [memory.rs](memory.rs) | Explicit project-local guidance, exact provenance and deterministic future-request rendering. |
| [permissions.rs](permissions.rs) | Inspectable effective workspace policy and bounded user restriction changes. |
| [review.rs](review.rs) | Structured diff targets, anchored feedback, and qualification-evidence freshness. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [brief/](brief/AGENTS.md) | Implementation pieces for `brief.rs`: User-confirmed brief projection with exact immutable input provenance. |
| [checkpoints/](checkpoints/AGENTS.md) | Implementation pieces for `checkpoints.rs`: Public bounded checkpoint coverage, rewind previews, and durable restore receipts. |
| [files/](files/AGENTS.md) | Implementation pieces for `files.rs`: Explicit workspace-file preview and selection. DTOs never confer read authority. |
| [goal/](goal/AGENTS.md) | Implementation pieces for `goal.rs`: Persistent-goal, safe-boundary, evidence, and usage DTOs. |
| [images/](images/AGENTS.md) | Implementation pieces for `images.rs`: Explicit image upload and preview contracts; none imply provider delivery or path authority. |
| [init/](init/AGENTS.md) | Implementation pieces for `init.rs`: Bounded, approval-first project initialization contracts. |
| [launch/](launch/AGENTS.md) | Implementation pieces for `launch.rs`: Typed launch profiles, scoped capture receipts, and independently classified result evidence. |
| [library/](library/AGENTS.md) | Implementation pieces for `library.rs`: Bounded conversation-library search and non-running fork contracts. |
| [memory/](memory/AGENTS.md) | Implementation pieces for `memory.rs`: Explicit project-local guidance, exact provenance and deterministic future-request rendering. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Structured diff targets, anchored feedback, and qualification-evidence freshness. |

Owning module: [workbench.rs](../workbench.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
