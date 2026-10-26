# crates/app/peritus-tui/src/render/product — navigation

## Responsibility

Implementation pieces for `product.rs`: Dashboard, diff, and review presentation for daemon-owned coding runs.

## Files in this folder

| File | Role |
| --- | --- |
| [detail.rs](detail.rs) | Run status, deliverable, conversation, phase, and scrollable text rendering. |
| [preview.rs](preview.rs) | Preview launch, capture, interaction, and human-feedback evidence rendering. |
| [structured_review.rs](structured_review.rs) | Digest-bound structured diff, anchored comments, and evidence freshness rendering. |

Owning module: [product.rs](../product.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
