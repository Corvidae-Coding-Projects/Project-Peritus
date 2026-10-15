# crates/app/peritus-product-runner/src/control — navigation

## Responsibility

Implementation pieces for `control.rs`: Pure durable workbench records and admission decisions, independent of A3 and presentation.

## Files in this folder

| File | Role |
| --- | --- |
| [branch.rs](branch.rs) | Durable conversation-branch lineage. |
| [brief.rs](brief.rs) | User-confirmed brief fields backed by exact immutable input revisions. |
| [checkpoint.rs](checkpoint.rs) | Bounded checkpoint lineage and restore-journal records without retained file bodies. |
| [compaction.rs](compaction.rs) | Deterministic C6-validated prompt-view reductions over immutable public replies. |
| [context.rs](context.rs) | Durable user context preferences over exact, already-admitted sources. |
| [files.rs](files.rs) | Durable exact file references; source descriptors are inert, never filesystem capabilities. |
| [goal.rs](goal.rs) | Durable goal state, cumulative accounting, and safe-boundary admission. |
| [guidance.rs](guidance.rs) | Typed inert audit payloads for daemon-owned project-guidance sidecars. |
| [identity.rs](identity.rs) | Nominal nonzero control identities, validated during construction and deserialization. |
| [images.rs](images.rs) | Explicit immutable image references and future-request selection, not filesystem grants. |
| [inputs.rs](inputs.rs) | Immutable input revisions and dependency-safe queue transitions, independent of transport. |
| [permissions.rs](permissions.rs) | Durable user restrictions intersected with immutable daemon host policy. |
| [record.rs](record.rs) | Revisioned conversation metadata and exact idempotent intents. |
| [reply.rs](reply.rs) | Public host-delivered replies, never provider reasoning or mutable prompt text. |
| [review.rs](review.rs) | Digest-bound conversational change feedback and hard path constraints. |
| [text.rs](text.rs) | Bounded inert public text with redacted Debug output. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [checkpoint/](checkpoint/AGENTS.md) | Implementation pieces for `checkpoint.rs`: Bounded checkpoint lineage and restore-journal records without retained file bodies. |
| [files/](files/AGENTS.md) | Implementation pieces for `files.rs`: Durable exact file references; source descriptors are inert, never filesystem capabilities. |
| [goal/](goal/AGENTS.md) | Implementation pieces for `goal.rs`: Durable goal state, cumulative accounting, and safe-boundary admission. |
| [identity/](identity/AGENTS.md) | Implementation pieces for `identity.rs`: Nominal nonzero control identities, validated during construction and deserialization. |
| [images/](images/AGENTS.md) | Implementation pieces for `images.rs`: Explicit immutable image references and future-request selection, not filesystem grants. |
| [inputs/](inputs/AGENTS.md) | Implementation pieces for `inputs.rs`: Immutable input revisions and dependency-safe queue transitions, independent of transport. |
| [record/](record/AGENTS.md) | Implementation pieces for `record.rs`: Revisioned conversation metadata and exact idempotent intents. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Digest-bound conversational change feedback and hard path constraints. |

Owning module: [control.rs](../control.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
