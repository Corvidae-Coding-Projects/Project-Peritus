# crates/app/peritus-tui/src/model/product — navigation

## Responsibility

Implementation pieces for `product.rs`: Interactive product-run composer and daemon-observation projection.

## Files in this folder

| File | Role |
| --- | --- |
| [composer.rs](composer.rs) | Follow-up modal composition with retained request drafts. |
| [interaction.rs](interaction.rs) | Product dashboard keyboard routing. |
| [observation.rs](observation.rs) | Polling and daemon-observation updates for the product-run screen. |
| [preview.rs](preview.rs) | Explicit preview launch, interaction, capture, and evidence inspection. |
| [resume.rs](resume.rs) | One-shot workspace-scoped selection for `peritus resume`. |
| [review.rs](review.rs) | Interactive structured-diff selection and revision-fenced comment controls. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [preview/](preview/AGENTS.md) | Implementation pieces for `preview.rs`: Explicit preview launch, interaction, capture, and evidence inspection. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Interactive structured-diff selection and revision-fenced comment controls. |

Owning module: [product.rs](../product.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
