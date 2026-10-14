# crates/app/peritus-tui/src/model/product/review — navigation

## Responsibility

Implementation pieces for `review.rs`: Interactive structured-diff selection and revision-fenced comment controls.

## Files in this folder

| File | Role |
| --- | --- |
| [diff_pages.rs](diff_pages.rs) | Bounded structured-diff page and raw-byte retrieval for review. |
| [editor.rs](editor.rs) | Review drafts retain the exact target selected when editing began. |
| [state.rs](state.rs) | Structured-diff review selection state. |

Owning module: [review.rs](../review.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
