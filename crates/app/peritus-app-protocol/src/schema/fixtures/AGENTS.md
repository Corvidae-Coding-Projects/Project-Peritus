# crates/app/peritus-app-protocol/src/schema/fixtures — navigation

## Responsibility

Implementation pieces for `fixtures.rs`: Deterministic classified A3 compatibility frames.

## Files in this folder

| File | Role |
| --- | --- |
| [brief.rs](brief.rs) | User-confirmed brief fixtures cover every field and all eligible latest-source lifecycles. |
| [checkpoints.rs](checkpoints.rs) | Covered-path checkpoint, preview-bound rewind, and durable restore fixtures. |
| [compaction.rs](compaction.rs) | Deterministic local compaction preview and explicit confirmation fixtures. |
| [context.rs](context.rs) | Stable content-free context query and projection fixtures. |
| [doctor.rs](doctor.rs) | Exact additive doctor request/report compatibility frames. |
| [files.rs](files.rs) | Exact file consent and retained version compatibility frames. |
| [goal.rs](goal.rs) | Persistent-goal fixtures cover query, every user intent, evidence, and unknown usage. |
| [images.rs](images.rs) | Scoped image transfer and provider-bound confirmation fixture coverage. |
| [init.rs](init.rs) | Approval-first initialization fixtures retain exact preconditions and unverified command argv. |
| [inputs.rs](inputs.rs) | Stable queue wire fixtures for each exact intent and the paginated public projection. |
| [interaction.rs](interaction.rs) | Checked additive conversation and catalog compatibility frames. |
| [launch.rs](launch.rs) | Exact launch, interaction, selected-window capture, and independent evidence frames. |
| [library.rs](library.rs) | Conversation-library and exact fork compatibility fixtures. |
| [memory.rs](memory.rs) | Explicit project-guidance query, mutation, and lifecycle compatibility frames. |
| [permissions.rs](permissions.rs) | Permission query, mutation and effective-policy compatibility frames. |
| [product.rs](product.rs) | Product-operation fixture values. |
| [review.rs](review.rs) | Structured-review query, page, and anchored mutation fixtures. |
| [run_observations.rs](run_observations.rs) | Exact candidate state alongside active work in canonical collection fixtures. |
| [values.rs](values.rs) | Checked source values for valid compatibility frames. |
| [workbench.rs](workbench.rs) | Exact additive control frames covering every initial intent and projection. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [brief/](brief/AGENTS.md) | Implementation pieces for `brief.rs`: User-confirmed brief fixtures cover every field and all eligible latest-source lifecycles. |
| [images/](images/AGENTS.md) | Implementation pieces for `images.rs`: Scoped image transfer and provider-bound confirmation fixture coverage. |

Owning module: [fixtures.rs](../fixtures.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
