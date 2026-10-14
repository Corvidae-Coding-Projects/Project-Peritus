# crates/app/peritus-app-protocol/src/workbench/images — navigation

## Responsibility

Implementation pieces for `images.rs`: Explicit image upload and preview contracts; none imply provider delivery or path authority.

## Files in this folder

| File | Role |
| --- | --- |
| [metadata.rs](metadata.rs) | Bounded raster metadata; a decoded DTO is not an image-validation proof. |
| [page.rs](page.rs) | Bounded, revision-fenced inspection of retained image references and current eligibility. |
| [preview.rs](preview.rs) | Provider-bound local preview. Confirmation revalidates this exact immutable observation. |

Owning module: [images.rs](../images.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
